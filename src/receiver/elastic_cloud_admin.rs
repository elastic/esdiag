// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{
    DataSource, DiagnosticManifest, ElasticsearchCluster, ManifestBuilder, SourceContext,
};
use super::{Receive, ReceiveRaw};
use crate::data::{Auth, KnownHost};
use eyre::Result;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION};
use reqwest::{Client, ClientBuilder, header::HeaderMap};
use serde::de::DeserializeOwned;
use url::Url;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;

const ELASTIC_CLOUD_ADMIN_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const ELASTIC_CLOUD_ADMIN_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ElasticCloudAdminReceiver {
    client: Client,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
}

impl ElasticCloudAdminReceiver {
    pub fn new(url: Url, api_key: String) -> Result<Self> {
        let mut default_headers = HeaderMap::new();
        default_headers.append("X-Management-Request", "true".parse().unwrap());
        default_headers.append(ACCEPT, "application/json".parse().unwrap());
        default_headers.append(ACCEPT_ENCODING, "gzip, deflate".parse().unwrap());
        default_headers.append(
            AUTHORIZATION,
            format!("ApiKey {}", api_key).parse().unwrap(),
        );
        let client = ClientBuilder::new()
            .default_headers(default_headers)
            .connect_timeout(ELASTIC_CLOUD_ADMIN_CONNECT_TIMEOUT)
            .timeout(ELASTIC_CLOUD_ADMIN_REQUEST_TIMEOUT)
            .build()?;
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
                let url = self.url.join(&format!("{}/", self.url.path()))?;
                let response = self.client.get(url).send().await?;
                let bytes = response.bytes().await?;
                let cluster: serde_json::Value = serde_json::from_slice(&bytes)?;
                let version_str = cluster
                    .get("version")
                    .and_then(|v| v.get("number"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| eyre::eyre!("No version found in root response"))?;
                semver::Version::parse(version_str)
                    .map_err(|e| eyre::eyre!("Failed to parse version: {}", e))
            })
            .await
    }
}

impl TryFrom<KnownHost> for ElasticCloudAdminReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        match host.get_auth()? {
            Auth::Apikey(apikey) => Ok(ElasticCloudAdminReceiver::new(url, apikey)?),
            _ => Err(eyre::eyre!("Elastic Cloud Admin requires a URL and ApiKey")),
        }
    }
}

impl Receive for ElasticCloudAdminReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        tracing::debug!(
            "Testing Elastic Cloud Admin connection to {}",
            self.url.as_str()
        );
        // An empty request to `/`
        let response = self.client.get(self.url.as_str()).send().await;

        match response {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    tracing::debug!("Elastic Cloud Admin connection successful: {}", status);
                    true
                } else {
                    tracing::error!(
                        "Elastic Cloud Admin connection to {} failed: {}",
                        self.url.as_str(),
                        status
                    );
                    false
                }
            }
            Err(e) => {
                tracing::error!(
                    "Elastic Cloud Admin connection to {} failed: {e}",
                    self.url.as_str()
                );
                if e.is_connect() || e.is_timeout() {
                    tracing::warn!(
                        "Elastic Cloud Admin and Elastic GovCloud Admin hosts require VPN access; verify your VPN connection and retry."
                    );
                }
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
        let source_path = T::resolve_source_request_path(&ctx)?;
        // Prepend the API proxy base path
        let path = match source_path.as_str() {
            "/" => format!("{}/", self.url.path()),
            _ => format!("{}/{}", self.url.path(), source_path),
        };
        let url = self.url.join(&path)?;
        tracing::debug!("Getting API: {}", url);
        let response = self.client.get(url).send().await?;

        tracing::debug!("Get Response: {:?}", response);

        let bytes = response.bytes().await?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let collection_date = chrono::Utc::now().to_rfc3339();
        tracing::info!("Creating diagnostic manifest with collection date {collection_date}");
        let cluster = self.get::<ElasticsearchCluster>().await?;
        let manifest = ManifestBuilder::from(cluster)
            .runner("esdiag")
            .collection_date(collection_date)
            .build();
        Ok(manifest.into())
    }
}

impl ReceiveRaw for ElasticCloudAdminReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let ctx = SourceContext::new("elasticsearch", self.get_version().await.ok().cloned());
        let source_path = T::resolve_source_request_path(&ctx)?;
        // Prepend the API proxy base path
        let path = match source_path.as_str() {
            "/" => format!("{}/", self.url.path()),
            _ => format!("{}/{}", self.url.path(), source_path),
        };
        let url = self.url.join(&path)?;
        tracing::debug!("Getting API: {}", url);
        let response = self.client.get(url).send().await?;

        // Return raw text
        response.text().await.map_err(Into::into)
    }
}

impl std::fmt::Display for ElasticCloudAdminReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elastic Cloud {}", self.url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State,
        Router,
        http::StatusCode,
        routing::get,
    };
    use tokio::task::JoinHandle;

    async fn status_handler(State(status): State<StatusCode>) -> (StatusCode, &'static str) {
        (status, "test response")
    }

    async fn spawn_status_server(status: StatusCode) -> (Url, JoinHandle<()>) {
        let app = Router::new()
            .route("/", get(status_handler))
            .with_state(status);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test app");
        });
        let url = Url::parse(&format!("http://{addr}/")).expect("parse test url");
        (url, server)
    }

    async fn stop_status_server(server: JoinHandle<()>) {
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn is_connected_returns_true_for_success_status() {
        let (url, server) = spawn_status_server(StatusCode::OK).await;
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        assert!(receiver.is_connected().await);

        stop_status_server(server).await;
    }

    #[tokio::test]
    async fn is_connected_returns_false_for_unauthorized_status() {
        let (url, server) = spawn_status_server(StatusCode::UNAUTHORIZED).await;
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        assert!(!receiver.is_connected().await);

        stop_status_server(server).await;
    }
}
