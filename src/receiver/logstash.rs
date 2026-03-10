// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{DataSource, DiagnosticManifest, SourceContext};
use super::{Receive, ReceiveRaw};
use crate::client::LogstashClient;
use crate::data::{KnownHost, Product};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::OnceCell;
use url::Url;

#[derive(Clone)]
pub struct LogstashReceiver {
    client: LogstashClient,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
}

impl LogstashReceiver {
    pub async fn get_version(&self) -> Result<&semver::Version> {
        self.version
            .get_or_try_init(|| async {
                log::debug!("Fetching Logstash version from {}", self.url);
                let response = self
                    .client
                    .request(Method::GET, &HashMap::new(), "/", None)
                    .await?;
                let body: Value = response.json().await?;
                let version_str = body
                    .get("version")
                    .and_then(|version| version.as_str())
                    .ok_or_else(|| eyre!("No version found in Logstash root response"))?;
                semver::Version::parse(version_str)
                    .map_err(|e| eyre!("Failed to parse Logstash version: {}", e))
            })
            .await
    }

    pub async fn get_raw_by_path(&self, path: &str, extension: &str) -> Result<String> {
        log::debug!("Getting raw Logstash API path: {}", path);

        let accept = if extension == ".txt" {
            "text/plain"
        } else {
            "application/json"
        };
        let headers = HashMap::from([("Accept".to_string(), accept.to_string())]);
        let response = self.client.request(Method::GET, &headers, path, None).await?;

        response.text().await.map_err(Into::into)
    }
}

impl TryFrom<KnownHost> for LogstashReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = LogstashClient::try_from(host)?;
        Ok(Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
        })
    }
}

impl Receive for LogstashReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        log::debug!("Testing Logstash receiver connection");
        self.client.test_connection().await.is_ok()
    }

    fn filename(&self) -> Option<String> {
        None
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let ctx = SourceContext::new("logstash", self.get_version().await.ok().cloned());
        let path = T::resolve_source_request_path(&ctx)?;
        log::debug!("Getting Logstash API: {}", &path);

        let response = self
            .client
            .request(Method::GET, &HashMap::new(), &path, None)
            .await?;

        match response.status() {
            reqwest::StatusCode::OK => response.json::<T>().await.map_err(Into::into),
            status => {
                let body = response.text().await.unwrap_or_default();
                log::debug!("Failed to get Logstash API response: {}", body);
                Err(eyre!("http {} - {}", status, body))
            }
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: super::super::processor::StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        Err(eyre!("Streaming is not yet implemented for Logstash receiver"))
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let version = self.get_version().await?;
        Ok(DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339(),
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            None,
            Product::Logstash,
            Some("logstash_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(version.to_string()),
        ))
    }
}

impl ReceiveRaw for LogstashReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let ctx = SourceContext::new("logstash", self.get_version().await.ok().cloned());
        let path = T::resolve_source_request_path(&ctx)?;
        let extension = T::resolve_source_extension(&ctx)?;
        self.get_raw_by_path(&path, &extension).await
    }
}

impl std::fmt::Display for LogstashReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Logstash {}", self.url)
    }
}
