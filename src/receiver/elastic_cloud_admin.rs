// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{
    DataSource, DiagnosticManifest, ElasticsearchCluster, ManifestBuilder, SourceContext,
};
use super::{Receive, ReceiveRaw};
use crate::data::{Auth, KnownHost};
use eyre::{Result, eyre};
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION};
use reqwest::{Client, ClientBuilder, header::HeaderMap};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;

const ELASTIC_CLOUD_ADMIN_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const ELASTIC_CLOUD_ADMIN_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub struct ElasticCloudAdminRequestError {
    pub status: reqwest::StatusCode,
    pub body: String,
}

impl std::fmt::Display for ElasticCloudAdminRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "http {} - {}", self.status, self.body)
    }
}

impl std::error::Error for ElasticCloudAdminRequestError {}

#[derive(Clone)]
pub struct ElasticCloudAdminReceiver {
    client: Client,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
}

impl ElasticCloudAdminReceiver {
    fn format_error_body(body: &str) -> String {
        serde_json::from_str::<Value>(body)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| body.to_string())
    }

    fn proxy_url(&self, path: &str) -> Result<Url> {
        let base_path = self.url.path().trim_end_matches('/');
        let relative_path = path.trim_start_matches('/');
        let proxy_path = if relative_path.is_empty() {
            format!("{base_path}/")
        } else {
            format!("{base_path}/{relative_path}")
        };
        self.url
            .join(&proxy_path)
            .map_err(|err| eyre!("Failed to resolve Elastic Cloud Admin URL: {err}"))
    }

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
                let url = self.proxy_url("/")?;
                let response = self.client.get(url).send().await?;
                let status = response.status();
                let body = response.text().await?;
                if !status.is_success() {
                    return Err(ElasticCloudAdminRequestError {
                        status,
                        body: Self::format_error_body(&body),
                    }
                    .into());
                }
                let cluster: serde_json::Value = serde_json::from_str(&body)?;
                let version_str = cluster
                    .get("version")
                    .and_then(|v| v.get("number"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| eyre::eyre!("No version found in root response"))?;
                semver::Version::parse(version_str)
                    .map_err(|e| eyre!("Failed to parse version: {}", e))
            })
            .await
    }

    pub async fn get_raw_by_path(&self, path: &str, extension: &str) -> Result<String> {
        tracing::debug!("Getting raw Elastic Cloud Admin API path: {}", path);

        let accept = if extension == ".txt" {
            "text/plain"
        } else {
            "application/json"
        };
        let url = self.proxy_url(path)?;
        let response = self.client.get(url).header(ACCEPT, accept).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ElasticCloudAdminRequestError {
                status,
                body: Self::format_error_body(&body),
            }
            .into());
        }

        Ok(body)
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
        let url = self.proxy_url(&source_path)?;
        tracing::debug!("Getting API: {}", url);
        let response = self.client.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ElasticCloudAdminRequestError {
                status,
                body: Self::format_error_body(&body),
            }
            .into());
        }

        serde_json::from_str(&body).map_err(Into::into)
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
        self.get_raw_by_path(&source_path, ".json").await
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
        extract::Request,
        extract::State,
        Router,
        http::StatusCode,
        middleware::{self, Next},
        routing::get,
    };
    use reqwest::header::ACCEPT;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;

    async fn status_handler(State(status): State<StatusCode>) -> (StatusCode, &'static str) {
        (status, "test response")
    }

    async fn spawn_status_server(status: StatusCode) -> (Url, JoinHandle<()>, oneshot::Sender<()>) {
        let app = Router::new()
            .route("/", get(status_handler))
            .with_state(status);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("serve test app");
        });
        let url = Url::parse(&format!("http://{addr}/")).expect("parse test url");
        (url, server, shutdown_tx)
    }

    async fn stop_status_server(server: JoinHandle<()>, shutdown_tx: oneshot::Sender<()>) {
        let _ = shutdown_tx.send(());
        server.await.expect("test server should exit cleanly");
    }

    async fn stop_status_server_with_shutdown(
        server: JoinHandle<()>,
        shutdown_tx: oneshot::Sender<()>,
    ) {
        let _ = shutdown_tx.send(());
        server.await.expect("test server should exit cleanly");
    }

    async fn capture_accept_header(
        State(last_accept): State<Arc<Mutex<Option<String>>>>,
        request: Request,
        next: Next,
    ) -> impl axum::response::IntoResponse {
        let header = request
            .headers()
            .get(ACCEPT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        *last_accept.lock().expect("accept lock") = header;
        next.run(request).await
    }

    #[tokio::test]
    async fn is_connected_returns_true_for_success_status() {
        let (url, server, shutdown_tx) = spawn_status_server(StatusCode::OK).await;
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        assert!(receiver.is_connected().await);

        stop_status_server(server, shutdown_tx).await;
    }

    #[tokio::test]
    async fn is_connected_returns_false_for_unauthorized_status() {
        let (url, server, shutdown_tx) = spawn_status_server(StatusCode::UNAUTHORIZED).await;
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        assert!(!receiver.is_connected().await);

        stop_status_server(server, shutdown_tx).await;
    }

    #[tokio::test]
    async fn get_raw_by_path_uses_proxy_base_path_and_accept_header() {
        async fn text_handler() -> &'static str {
            "hot threads"
        }

        let last_accept = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route(
                "/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/_nodes/hot_threads",
                get(text_handler),
            )
            .layer(middleware::from_fn_with_state(
                last_accept.clone(),
                capture_accept_header,
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("serve test app");
        });
        let url = Url::parse(&format!(
            "http://{addr}/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/"
        ))
        .expect("parse test url");
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        let body = receiver
            .get_raw_by_path("/_nodes/hot_threads", ".txt")
            .await
            .expect("raw response");

        assert_eq!(body, "hot threads");
        assert_eq!(
            last_accept.lock().expect("accept lock").as_deref(),
            Some("text/plain")
        );

        stop_status_server_with_shutdown(server, shutdown_tx).await;
    }

    #[tokio::test]
    async fn get_raw_by_path_returns_request_error_for_http_failures() {
        async fn not_found_handler() -> (StatusCode, &'static str) {
            (StatusCode::NOT_FOUND, r#"{"error":"missing"}"#)
        }

        let app = Router::new().route(
            "/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/_cluster/missing",
            get(not_found_handler),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("listener addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("serve test app");
        });
        let url = Url::parse(&format!(
            "http://{addr}/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/"
        ))
        .expect("parse test url");
        let receiver =
            ElasticCloudAdminReceiver::new(url, "test-api-key".to_string()).expect("receiver");

        let err = receiver
            .get_raw_by_path("/_cluster/missing", ".json")
            .await
            .expect_err("should fail");
        let request_error = err
            .downcast_ref::<ElasticCloudAdminRequestError>()
            .expect("request error");

        assert_eq!(request_error.status, StatusCode::NOT_FOUND);
        assert_eq!(request_error.body, r#"{"error":"missing"}"#);

        stop_status_server_with_shutdown(server, shutdown_tx).await;
    }
}
