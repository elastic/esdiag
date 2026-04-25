// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{
    DataSource, DiagnosticManifest, ElasticsearchCluster, ManifestBuilder, SourceContext, StreamingDataSource,
};
use super::{RawResponse, Receive, ReceiveRaw};
use crate::data::KnownHost;
use elasticsearch::{Elasticsearch, http};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::OnceCell;

#[derive(Debug)]
pub struct ElasticsearchRequestError {
    pub status: http::StatusCode,
    pub body: String,
    pub response_time_ms: u64,
    pub response_size_bytes: u64,
}

impl std::fmt::Display for ElasticsearchRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "http {} - {}", self.status, self.body)
    }
}

impl std::error::Error for ElasticsearchRequestError {}

#[derive(Clone)]
pub struct ElasticsearchReceiver {
    client: Elasticsearch,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
}

impl ElasticsearchReceiver {
    fn format_error_body(body: &str) -> String {
        serde_json::from_str::<Value>(body)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| body.to_string())
    }

    pub fn new(url: Url) -> Result<Self> {
        let client = Elasticsearch::default();
        Ok(Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
        })
    }

    pub async fn get_version(&self) -> Result<&semver::Version> {
        self.version
            .get_or_try_init(|| async {
                tracing::debug!("Fetching version from {}", self.url);
                let started = Instant::now();
                let response = self
                    .client
                    .send(
                        http::Method::Get,
                        "",
                        http::headers::HeaderMap::new(),
                        Option::<&String>::None,
                        Option::<&String>::None,
                        None,
                    )
                    .await?;
                let status = response.status_code();
                let body = response.text().await?;
                let response_time_ms = started.elapsed().as_millis() as u64;
                let response_size_bytes = body.len() as u64;
                if !status.is_success() {
                    return Err(ElasticsearchRequestError {
                        status,
                        body: Self::format_error_body(&body),
                        response_time_ms,
                        response_size_bytes,
                    }
                    .into());
                }
                let cluster: Value = serde_json::from_str(&body)?;
                let version_str = cluster
                    .get("version")
                    .and_then(|version| version.get("number").and_then(|number| number.as_str()))
                    .ok_or_else(|| eyre!("No version found in root response"))?;
                semver::Version::parse(version_str).map_err(|e| eyre!("Failed to parse version: {}", e))
            })
            .await
    }

    pub async fn get_raw_response_by_path(&self, path: &str, extension: &str) -> Result<RawResponse> {
        tracing::debug!("Getting raw API path: {}", path);
        let started = Instant::now();

        let mut headers = http::headers::HeaderMap::new();
        // By default, the Elasticsearch client enforces Accept: application/json
        // We use the configured file extension to request the appropriate content type
        if extension == ".txt" {
            headers.append(http::headers::ACCEPT, "text/plain".parse().unwrap());
        } else {
            headers.append(http::headers::ACCEPT, "application/json".parse().unwrap());
        }

        let response = self
            .client
            .send(
                http::Method::Get,
                path,
                headers,
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await?;
        let status = response.status_code();
        let body = response.text().await?;
        let response_time_ms = started.elapsed().as_millis() as u64;
        let response_size_bytes = body.len() as u64;
        if !status.is_success() {
            return Err(ElasticsearchRequestError {
                status,
                body: Self::format_error_body(&body),
                response_time_ms,
                response_size_bytes,
            }
            .into());
        }

        Ok(RawResponse {
            body,
            status: status.as_u16(),
            response_time_ms,
            response_size_bytes,
        })
    }

    pub async fn get_raw_by_path(&self, path: &str, extension: &str) -> Result<String> {
        self.get_raw_response_by_path(path, extension)
            .await
            .map(|response| response.body)
    }
}

impl TryFrom<KnownHost> for ElasticsearchReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = Elasticsearch::try_from(host)?;
        Ok(Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
        })
    }
}

impl Receive for ElasticsearchReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        tracing::debug!("Testing Elasticsearch client connection");
        // An empty request to `/`
        let response = self
            .client
            .send(
                http::Method::Get,
                "",
                http::headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await;

        match response {
            Ok(response) => {
                tracing::debug!("Elasticsearch client connection successful: {}", response.status_code());
                true
            }
            Err(e) => {
                tracing::error!("Elasticsearch client connection failed: {e}");
                false
            }
        }
    }

    fn filename(&self) -> Option<String> {
        None
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let ctx = SourceContext::new("elasticsearch", self.get_version().await.ok().cloned());
        let path = T::resolve_source_request_path(&ctx)?;
        tracing::debug!("Getting API: {}", &path);

        // Send a simple GET request to the API path
        let started = Instant::now();
        let response = self
            .client
            .send(
                http::Method::Get,
                &path,
                http::headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await?;
        match response.status_code() {
            http::StatusCode::OK => response.json::<T>().await.map_err(Into::into),
            status => {
                let body = response.text().await?;
                let formatted_body = Self::format_error_body(&body);
                let response_time_ms = started.elapsed().as_millis() as u64;
                let response_size_bytes = body.len() as u64;
                tracing::debug!("Failed to get API response: {}", formatted_body);
                Err(ElasticsearchRequestError {
                    status,
                    body: formatted_body,
                    response_time_ms,
                    response_size_bytes,
                }
                .into())
            }
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        // TODO: Implement proper streaming from Elasticsearch response body
        // The elasticsearch-rs client currently doesn't easily expose a streaming response body
        // compatible with serde_json::Deserializer.
        Err(eyre!("Streaming is not yet implemented for Elasticsearch receiver"))
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let collection_date = chrono::Utc::now().to_rfc3339();
        tracing::info!("Creating diagnostic manifest with collection date {collection_date}");
        let cluster = match self.get::<ElasticsearchCluster>().await {
            Ok(cluster) => cluster,
            Err(err) => {
                tracing::debug!("Failed to get Elasticsearch cluster info: {}", err);
                return Err(err);
            }
        };
        let manifest = ManifestBuilder::from(cluster)
            .runner("esdiag")
            .collection_date(collection_date)
            .build();
        Ok(manifest.into())
    }
}

impl ReceiveRaw for ElasticsearchReceiver {
    async fn get_raw_response<T>(&self) -> Result<RawResponse>
    where
        T: DataSource,
    {
        let ctx = SourceContext::new("elasticsearch", self.get_version().await.ok().cloned());
        let path = T::resolve_source_request_path(&ctx)?;
        let extension = T::resolve_source_extension(&ctx)?;

        self.get_raw_response_by_path(&path, &extension).await
    }
}

impl std::fmt::Display for ElasticsearchReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elasticsearch {}", self.url)
    }
}
