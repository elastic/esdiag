// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{DataSource, DiagnosticManifest, SourceContext, StreamingDataSource};
use super::{Receive, ReceiveRaw};
use crate::{
    client::KibanaClient,
    data::{KnownHost, Product},
};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::OnceCell;
use url::Url;

#[derive(Clone)]
pub struct KibanaReceiver {
    client: KibanaClient,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
    spaces: Arc<OnceCell<Vec<String>>>,
}

#[derive(Deserialize)]
struct KibanaStatusVersion {
    number: String,
}

#[derive(Deserialize)]
struct KibanaStatusResponse {
    version: KibanaStatusVersion,
}

#[derive(Deserialize)]
struct KibanaSpace {
    id: String,
}

#[derive(Debug)]
pub struct KibanaRequestError {
    pub status: reqwest::StatusCode,
    pub body: String,
}

impl std::fmt::Display for KibanaRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "http {} - {}", self.status, self.body)
    }
}

impl std::error::Error for KibanaRequestError {}

impl KibanaReceiver {
    pub fn new(url: Url, client: KibanaClient) -> Self {
        Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
            spaces: Arc::new(OnceCell::new()),
        }
    }

    async fn get_status(&self) -> Result<KibanaStatusResponse> {
        let response = self
            .client
            .request(Method::GET, &HashMap::new(), "/api/status", None)
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(KibanaRequestError { status, body }.into());
        }
        serde_json::from_str(&body).map_err(Into::into)
    }

    pub async fn get_version(&self) -> Result<&semver::Version> {
        self.version
            .get_or_try_init(|| async {
                let status = self.get_status().await?;
                semver::Version::parse(&status.version.number)
                    .map_err(|e| eyre!("Failed to parse Kibana version: {}", e))
            })
            .await
    }

    pub async fn get_spaces(&self) -> Result<&Vec<String>> {
        self.spaces
            .get_or_try_init(|| async {
                let response = self
                    .client
                    .request(Method::GET, &HashMap::new(), "/api/spaces/space", None)
                    .await?;
                let status = response.status();
                let body = response.text().await?;
                if !status.is_success() {
                    return Err(KibanaRequestError { status, body }.into());
                }
                let spaces: Vec<KibanaSpace> = serde_json::from_str(&body)?;
                Ok(spaces.into_iter().map(|space| space.id).collect())
            })
            .await
    }

    pub async fn get_raw_by_path(&self, path: &str, extension: &str) -> Result<String> {
        tracing::debug!("Getting raw Kibana API path: {}", path);

        let mut headers = HashMap::new();
        if extension == ".txt" {
            headers.insert("Accept".to_string(), "text/plain".to_string());
        } else {
            headers.insert("Accept".to_string(), "application/json".to_string());
        }

        let response = self
            .client
            .request(Method::GET, &headers, path, None)
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(KibanaRequestError { status, body }.into());
        }
        Ok(body)
    }
}

impl TryFrom<KnownHost> for KibanaReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = KibanaClient::try_from(host)?;
        Ok(Self::new(url, client))
    }
}

impl Receive for KibanaReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        self.client.test_connection().await.is_ok()
    }

    fn filename(&self) -> Option<String> {
        None
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        let ctx = SourceContext::new("kibana", Some(self.get_version().await?.clone()));
        let path = T::resolve_source_request_path(&ctx)?;
        let response = self
            .client
            .request(Method::GET, &HashMap::new(), &path, None)
            .await?;
        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            serde_json::from_str(&body).map_err(Into::into)
        } else {
            let body_json = serde_json::from_str::<Value>(&body).unwrap_or(Value::String(body));
            Err(KibanaRequestError {
                status,
                body: body_json.to_string(),
            }
            .into())
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        Err(eyre!(
            "Streaming is not yet implemented for Kibana receiver"
        ))
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let status = self.get_status().await?;
        Ok(DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339(),
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some("compatible".to_string()),
            Product::Kibana,
            Some("kibana_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(status.version.number),
        ))
    }
}

impl ReceiveRaw for KibanaReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let ctx = SourceContext::new("kibana", Some(self.get_version().await?.clone()));
        let path = T::resolve_source_request_path(&ctx)?;
        let extension = T::resolve_source_extension(&ctx)?;
        self.get_raw_by_path(&path, &extension).await
    }
}

impl std::fmt::Display for KibanaReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Kibana {}", self.url)
    }
}
