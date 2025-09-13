// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::Export;
use crate::{
    client::{Auth, ElasticsearchBuilder, KnownHost},
    data,
    processor::{BatchResponse, DiagnosticReport, Identifiers, ProcessorSummary},
};
use elasticsearch::{
    BulkOperation, BulkParts, Elasticsearch, IndexParts,
    http::{Method, headers, request::JsonBody, response::Response},
};
use eyre::{Result, eyre};
use futures::stream::FuturesUnordered;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use url::Url;

/// An exporter that sends documents to an Elasticsearch cluster.
#[derive(Clone)]
pub struct ElasticsearchExporter {
    client: Elasticsearch,
    url: Url,
    pub identifiers: Identifiers,
}

impl ElasticsearchExporter {
    /// Create a new ElasticsearchExporter from a URL and Auth
    pub fn new(url: Url, auth: Auth) -> Result<Self> {
        let client = ElasticsearchBuilder::new(url.clone())
            .insecure(true)
            .auth(auth)
            .build()?;

        Ok(Self {
            client,
            url,
            identifiers: Identifiers::default(),
        })
    }

    /// Request to an arbitrary path on the Elasticsearch client
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        value: Option<&Value>,
    ) -> Result<Response> {
        let method = match method {
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            _ => Method::Get,
        };
        let body = match value {
            Some(value) => Some(JsonBody::new(value)),
            None => None,
        };
        self.client
            .send(
                method,
                path,
                headers::HeaderMap::new(),
                Option::<&Value>::None,
                body,
                None,
            )
            .await
            .map_err(|e| e.into())
    }
}

impl TryFrom<KnownHost> for ElasticsearchExporter {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = Elasticsearch::try_from(host)?;
        Ok(Self {
            client,
            url,
            identifiers: Identifiers::default(),
        })
    }
}

impl Export for ElasticsearchExporter {
    /// Adds identifiers to the exporter, which will be enriched on every document sent.
    fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers,
            ..self
        }
    }

    /// Check if the exporter has a valid connection to Elasticsearch.
    async fn is_connected(&self) -> bool {
        let status_code = match self.client.info().send().await {
            Ok(res) => {
                log::debug!("Exporter is connected: {}", res.status_code());
                log::trace!("{:?}", res);
                res.status_code().as_u16()
            }
            Err(e) => {
                log::error!("{e}");
                599
            }
        };

        status_code == 200
    }

    /// Drains the docs array into batches and sends them to Elasticsearch with multiple workers.
    async fn send<T>(&self, summary: &mut ProcessorSummary, docs: &mut Vec<T>) -> Result<()>
    where
        T: Serialize + Sized + Send + Sync,
    {
        use futures::{FutureExt, StreamExt};
        let client = self.client.clone();
        let workers = 4;
        let bulk_size = 5_000;

        let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();

        while !docs.is_empty() || !in_flight.is_empty() {
            while in_flight.len() < workers && !docs.is_empty() {
                let batch_size = docs.len().min(bulk_size);
                let mut batch: Vec<BulkOperation<T>> = Vec::with_capacity(batch_size);
                batch.extend(
                    docs.drain(..batch_size)
                        .map(|doc| BulkOperation::create(doc).pipeline("esdiag").into()),
                );
                let batch_index = summary.index.clone();
                let batch_client = client.clone();
                let fut = async move {
                    let response = batch_client
                        .bulk(BulkParts::Index(&batch_index))
                        .body(batch)
                        .send()
                        .await;
                    parse_response(batch_index, response).await
                };
                in_flight.push(fut.boxed());
            }

            // Only create the next batch after one has completed
            if let Some(res) = in_flight.next().await {
                match res {
                    Ok(batch_response) => summary.add_batch(batch_response),
                    Err(e) => {
                        log::warn!("Bulk batch failed: {e:?}");
                    }
                }
            }
        }
        Ok(())
    }

    /// Transmits a single batch of documents in an async task
    /// Returns a one-shot channel for the BatchResponse
    async fn tx<T>(&self, index: String, docs: Vec<T>) -> Result<oneshot::Receiver<BatchResponse>>
    where
        T: Serialize + Sized + Send + Sync + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let client = self.client.clone();

        tokio::spawn(async move {
            let batch: Vec<BulkOperation<T>> = docs
                .into_iter()
                .map(|doc| BulkOperation::create(doc).pipeline("esdiag").into())
                .collect();

            let response = client
                .bulk(BulkParts::Index(&index))
                .body(batch)
                .send()
                .await;

            match parse_response(index, response).await {
                Ok(batch_response) => {
                    if tx.send(batch_response).is_err() {
                        log::error!("Failed to send batch response: receiver dropped");
                    }
                }
                Err(e) => {
                    log::warn!("Bulk batch failed: {}", e);
                }
            }
        });

        Ok(rx)
    }

    /// Sends the final diagnostic report document to Elasticsearch.
    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        data::save_file("report.json", report)?;
        let diagnostic_id = report.metadata.id.clone();
        let body = json!({
            "@timestamp": chrono::Utc::now().timestamp_millis(),
            "diagnostic": report ,
            "agent": {
                "type": "esdiag",
                "version": semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
            }
        });
        match self
            .client
            .index(IndexParts::Index("metrics-diagnostic-esdiag"))
            .pipeline("esdiag")
            .body(body)
            .send()
            .await
        {
            Ok(res) => {
                let status_code = res.status_code().as_u16();
                let body = res.json::<Value>().await?;
                match status_code {
                    200 | 201 => {
                        log::info!(
                            "metrics-diagnostic-esdiag, created diagnostic report {}",
                            diagnostic_id
                        );
                        log::trace!("response body: {body}");
                        Ok(())
                    }
                    400..600 => Err(eyre!("http {status_code}: {body}")),
                    _ => Err(eyre!("unexpected response: http {status_code}: {body}")),
                }
            }
            Err(e) => {
                log::error!("{e}");
                Err(e.into())
            }
        }
    }
}

impl std::fmt::Display for ElasticsearchExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

async fn parse_response(
    index: String,
    response: Result<Response, elasticsearch::Error>,
) -> Result<BatchResponse> {
    let response = response?;
    log::trace!("{:?}", &response);
    let status_code = response.status_code().as_u16();
    let body: Value = response.json().await?;
    let mut items: Vec<Value> = body["items"].as_array().unwrap_or(&Vec::new()).clone();
    let item_count = items.len();

    let error_items: Vec<Value> = items
        .drain(..)
        .filter(|item| match item["create"]["status"].as_u64() {
            Some(status) => status != 201,
            None => false,
        })
        .collect();
    let error_count = error_items.len();
    let doc_count = item_count - error_count;

    if (status_code != 200 && log::max_level() >= log::Level::Debug)
        || (log::max_level() >= log::Level::Trace)
    {
        data::save_file(
            "responses.ndjson",
            &serde_json::json!({
                "index": index,
                "doc_count": doc_count,
                "error_count": error_count,
                "errors": error_items,
                "body": body,
            }),
        )?;
    }

    match status_code {
        200 if error_count == 0 => log::debug!("{}, created {} docs", index, doc_count),
        200 => log::warn!(
            "{}, created {} docs with {} errors",
            index,
            doc_count,
            error_count
        ),
        400 => return Err(eyre!("{} - http 400 bad request", index)),
        401 => return Err(eyre!("{} - http 401 unauthorized", index)),
        403 => return Err(eyre!("{} - http 403 forbidden", index)),
        404 => return Err(eyre!("{} - http 404 not found", index)),
        413 => return Err(eyre!("{} - http 413 request too large", index)),
        429 => return Err(eyre!("{} - http 429 too many requests", index)),
        500..=599 => return Err(eyre!("{} - server errors: http {}", status_code, index)),
        _ => log::warn!("unexpected http response: {}", status_code),
    }

    if error_count > 0 {
        data::save_file(
            "bulk_errors.ndjson",
            &serde_json::json!({
                "index": index,
                "doc_count": doc_count,
                "error_count": error_count,
                "errors": error_items
            }),
        )?;
    }

    let batch_response = BatchResponse {
        docs: item_count as u32,
        errors: error_count as u32,
        retries: 0,
        size: 0,
        status_code,
        time: body.get("took").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    };

    Ok(batch_response)
}
