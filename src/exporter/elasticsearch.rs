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
use url::Url;

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

    /// Send a request to an arbitrary path on the Elasticsearch client
    pub async fn send(&self, method: &str, path: &str, value: Option<&Value>) -> Result<Response> {
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
    fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers,
            ..self
        }
    }

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

    async fn write<T>(&self, summary: &mut ProcessorSummary, docs: &mut Vec<T>) -> Result<()>
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
