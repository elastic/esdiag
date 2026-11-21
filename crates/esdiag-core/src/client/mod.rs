// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Builder for the Elasticsearch client
mod elasticsearch;
/// Client for Kibana APIs
mod kibana;
/// Client for Logstash APIs
mod logstash;

pub use elasticsearch::{ElasticsearchBuilder, ElasticsearchClient, elasticsearch_client_from_output_host};
pub(crate) use kibana::KIBANA_REQUEST_CONCURRENCY;
pub use kibana::KibanaClient;
pub use logstash::LogstashClient;

extern crate elasticsearch as es;
use crate::data::{Application, Uri};
use eyre::{Result, eyre};
use reqwest::Method;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

/// Preserves each client's response without depending on fork-only conversions.
pub enum ClientResponse {
    Elasticsearch(es::http::response::Response),
    Http(reqwest::Response),
}

impl ClientResponse {
    pub fn status(&self) -> reqwest::StatusCode {
        match self {
            Self::Elasticsearch(response) => {
                reqwest::StatusCode::from_u16(response.status_code().as_u16()).expect("valid HTTP status code")
            }
            Self::Http(response) => response.status(),
        }
    }

    pub async fn text(self) -> Result<String> {
        match self {
            Self::Elasticsearch(response) => Ok(response.text().await?),
            Self::Http(response) => Ok(response.text().await?),
        }
    }

    pub async fn bytes(self) -> Result<bytes::Bytes> {
        match self {
            Self::Elasticsearch(response) => Ok(response.bytes().await?),
            Self::Http(response) => Ok(response.bytes().await?),
        }
    }

    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        match self {
            Self::Elasticsearch(response) => Ok(response.json().await?),
            Self::Http(response) => Ok(response.json().await?),
        }
    }
}

// Inspect the whole transport error chain, but never expose URLs or credentials.
fn connection_failure(error: &(dyn std::error::Error + 'static)) -> String {
    let mut messages = Vec::new();
    let mut cause = Some(error);
    while let Some(error) = cause {
        messages.push(error.to_string().to_ascii_lowercase());
        cause = error.source();
    }
    let details = messages.join(" ");
    if [
        "dns error",
        "dns lookup",
        "failed to lookup address",
        "name or service not known",
        "could not resolve host",
        "nodename nor servname",
        "getaddrinfo",
    ]
    .iter()
    .any(|marker| details.contains(marker))
    {
        "DNS lookup failed"
    } else if details.contains("certificate") || details.contains("tls") || details.contains("ssl") {
        "TLS verification failed"
    } else if details.contains("timed out") || details.contains("timeout") {
        "connection timed out"
    } else {
        "connection failed"
    }
    .to_string()
}

/// A standardized client for interacting with Elastic Stack APIs
pub enum Client {
    Elasticsearch(ElasticsearchClient),
    Kibana(KibanaClient),
    Logstash(LogstashClient),
}

impl Client {
    /// Detect the deployment from product metadata, without relying on its hostname.
    pub async fn is_serverless(&self) -> Result<bool> {
        let path = match self {
            Client::Elasticsearch(_) => "/",
            Client::Kibana(_) => "/api/status",
            Client::Logstash(_) => return Ok(false),
        };
        let response = self.request(Method::GET, &HashMap::new(), path, None).await?;
        let status = response.status();
        if matches!(status.as_u16(), 401 | 403 | 404) {
            if matches!(self, Client::Elasticsearch(_)) {
                let usage = self
                    .request(Method::GET, &HashMap::new(), "/_xpack/usage", None)
                    .await?;
                if usage.status() == reqwest::StatusCode::GONE {
                    tracing::debug!(
                        "Deployment metadata is restricted and the security usage API is gone; identifying the deployment as Serverless"
                    );
                    return Ok(true);
                }
            }
            tracing::debug!("Deployment metadata is unavailable (HTTP {status}); using endpoint capability probes");
            return Ok(false);
        }
        if !status.is_success() {
            return Err(eyre!("Failed to detect deployment type: HTTP {status}"));
        }
        let metadata: serde_json::Value = response.json().await?;
        Ok(metadata
            .pointer("/version/build_flavor")
            .and_then(serde_json::Value::as_str)
            == Some("serverless"))
    }

    /// Send an HTTP request to a path on the client's base URL
    pub async fn request(
        &self,
        method: Method,
        headers: &HashMap<String, String>,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<ClientResponse> {
        tracing::debug!("Request: {method} {path}");
        match self {
            Client::Elasticsearch(client) => {
                let method = match method {
                    Method::GET => es::http::Method::Get,
                    Method::POST => es::http::Method::Post,
                    Method::PUT => es::http::Method::Put,
                    Method::DELETE => es::http::Method::Delete,
                    Method::HEAD => es::http::Method::Head,
                    _ => return Err(eyre!("Unsupported http method for Elasticsearch client")),
                };
                let header_map: es::http::headers::HeaderMap = headers
                    .iter()
                    .filter_map(|(k, v)| match (k.parse(), v.parse()) {
                        (Ok(k), Ok(v)) => Some((k, v)),
                        x => {
                            tracing::warn!("Failed to parse header: {:?}", x);
                            None
                        }
                    })
                    .collect();
                use es::http::request::JsonBody;
                let body: Option<JsonBody<serde_json::Value>> =
                    body.map(serde_json::from_slice).transpose()?.map(JsonBody::new);
                let response = client
                    .send(method, path, header_map, Option::<&serde_json::Value>::None, body, None)
                    .await?;
                Ok(ClientResponse::Elasticsearch(response))
            }
            Client::Kibana(client) => client
                .request(method, headers, path, body)
                .await
                .map(ClientResponse::Http),
            Client::Logstash(client) => client
                .request(method, headers, path, body)
                .await
                .map(ClientResponse::Http),
        }
    }

    /// Verify the connection and authentication to the stack component
    pub async fn test_connection(&self) -> std::result::Result<String, String> {
        match self {
            Client::Elasticsearch(client) => {
                let response = client
                    .send(
                        es::http::Method::Get,
                        "/",
                        es::http::headers::HeaderMap::new(),
                        Option::<&serde_json::Value>::None,
                        Option::<es::http::request::JsonBody<serde_json::Value>>::None,
                        None,
                    )
                    .await
                    .map_err(|e| connection_failure(&e))?;

                let status = response.status_code();
                if !status.is_success() {
                    return Err(format!("HTTP {status}"));
                }
                let json: serde_json::Value = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to read test body: {e}"))?;
                tracing::debug!("Test response {} ", json);

                if json.get("tagline").is_some() {
                    Ok(format!("{} ✅ Elasticsearch", status))
                } else {
                    Err(format!("{} ❌ Root response did not match Elasticsearch", status))
                }
            }
            Client::Kibana(client) => {
                let response = client
                    .test_connection()
                    .await
                    .map_err(|e| connection_failure(e.as_ref()))?;
                let status = response.status();
                if !status.is_success() {
                    return Err(format!("HTTP {status}"));
                }
                let json: serde_json::Value = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to read test body: {e}"))?;
                tracing::debug!("Test response {} ", json);
                match json.get("name") {
                    Some(name) => Ok(format!("{status} ✅ Kibana: {name}")),
                    None => Err(format!("{status} ❌ Host is not an Kibana node!")),
                }
            }
            Client::Logstash(client) => {
                let response = client
                    .test_connection()
                    .await
                    .map_err(|e| connection_failure(e.as_ref()))?;
                let status = response.status();
                if !status.is_success() {
                    return Err(format!("HTTP {status}"));
                }
                let json: serde_json::Value = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to read test body: {e}"))?;
                tracing::debug!("Test response {} ", json);

                if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
                    let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    Ok(format!("{status} ✅ Logstash: {name} ({version})"))
                } else {
                    Err(format!("{} ❌ Root response did not match Logstash", status))
                }
            }
        }
    }

    /// Check if security is enabled on the cluster.
    ///
    /// For Elasticsearch, this checks the `security.enabled` flag in `/_xpack/usage`.
    /// An unavailable usage API (HTTP 410 on Serverless) does not mean security is disabled.
    /// Serverless always has security enabled, including when the probe returns HTTP 410.
    /// For Kibana and Logstash, this currently always returns `true`.
    pub async fn has_security_enabled(&self) -> Result<bool> {
        match self {
            Client::Elasticsearch(client) => {
                let response = client
                    .send(
                        es::http::Method::Get,
                        "/_xpack/usage",
                        es::http::headers::HeaderMap::new(),
                        Option::<&serde_json::Value>::None,
                        Option::<es::http::request::JsonBody<serde_json::Value>>::None,
                        None,
                    )
                    .await?;

                let status = response.status_code();
                if status.is_success() {
                    let json: serde_json::Value = response.json().await?;
                    let enabled = json
                        .get("security")
                        .and_then(|s| s.get("enabled"))
                        .and_then(|e| e.as_bool())
                        .unwrap_or(true);
                    Ok(enabled)
                } else {
                    match status.as_u16() {
                        401 | 403 => {
                            tracing::debug!(
                                "Security detection returned {status}. Security is enabled but access to /_xpack/usage is restricted."
                            );
                            Ok(true)
                        }
                        404 => {
                            tracing::debug!(
                                "Security detection returned 404. Assuming security is disabled or not supported."
                            );
                            Ok(false)
                        }
                        410 => {
                            tracing::debug!(
                                "Serverless security is always enabled; the security usage API is unavailable."
                            );
                            Ok(true)
                        }
                        _ => {
                            tracing::warn!("Failed to check security status (HTTP {status}).");
                            Err(eyre!("Failed to check security status: HTTP {status}"))
                        }
                    }
                }
            }
            Client::Kibana(_) => {
                // For Kibana we assume true for now as requested
                Ok(true)
            }
            Client::Logstash(_) => {
                // Logstash does not expose the Elasticsearch security usage endpoint
                Ok(true)
            }
        }
    }
}

impl From<Client> for Application {
    fn from(client: Client) -> Self {
        match client {
            Client::Elasticsearch(_) => Application::Elasticsearch,
            Client::Kibana(_) => Application::Kibana,
            Client::Logstash(_) => Application::Logstash,
        }
    }
}

impl From<&Client> for Application {
    fn from(client: &Client) -> Self {
        match client {
            Client::Elasticsearch(_) => Application::Elasticsearch,
            Client::Kibana(_) => Application::Kibana,
            Client::Logstash(_) => Application::Logstash,
        }
    }
}

impl std::fmt::Display for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Client::Elasticsearch(_) => write!(f, "elasticsearch"),
            Client::Kibana(_) => write!(f, "kibana"),
            Client::Logstash(_) => write!(f, "logstash"),
        }
    }
}

impl TryFrom<Uri> for Client {
    type Error = eyre::Report;

    fn try_from(uri: Uri) -> Result<Self, Self::Error> {
        match uri {
            Uri::KnownHost(host) => {
                let resolved = host.resolve()?;
                let application = resolved.application();
                let host = resolved.into_known_host();
                match application {
                    Application::Kibana => Ok(Client::Kibana(KibanaClient::try_from(host)?)),
                    Application::Elasticsearch => Ok(Client::Elasticsearch(ElasticsearchClient::try_from(host)?)),
                    Application::Logstash => Ok(Client::Logstash(LogstashClient::try_from(host)?)),
                    Application::Agent => unreachable!("KnownHost::resolve returned Agent"),
                }
            }
            _ => Err(eyre!("Unsupported URI")),
        }
    }
}

#[cfg(test)]
mod connection_failure_tests {
    use super::connection_failure;

    #[test]
    fn classifies_nested_transport_errors_without_exposing_their_contents() {
        for (cause, expected) in [
            ("dns error: failed to lookup address", "DNS lookup failed"),
            ("windows connection failure", "connection failed"),
            ("invalid peer certificate", "TLS verification failed"),
            ("operation timed out", "connection timed out"),
            ("tcp connect error", "connection failed"),
        ] {
            let error = eyre::Report::new(std::io::Error::other(format!("{cause}: api_key=secret")))
                .wrap_err("Failed to send request");
            assert_eq!(connection_failure(error.as_ref()), expected);
        }
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;
    use axum::{Json, Router, http::StatusCode, routing::get};
    use serde_json::json;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn published_elasticsearch_response_preserves_status_and_body() {
        let app = Router::new().route(
            "/_compatibility",
            get(|| async { (StatusCode::CREATED, Json(json!({ "created": true }))) }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("local address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test application");
        });
        let client = Client::Elasticsearch(
            ElasticsearchBuilder::new(format!("http://{address}").parse().expect("server URL"))
                .build()
                .expect("Elasticsearch client"),
        );
        for body_format in ["json", "text", "bytes"] {
            let response = client
                .request(Method::GET, &HashMap::new(), "/_compatibility", None)
                .await
                .expect("Elasticsearch request");
            assert_eq!(response.status(), StatusCode::CREATED);
            let body: serde_json::Value = match body_format {
                "json" => response.json().await.expect("JSON response"),
                "text" => serde_json::from_str(&response.text().await.expect("text response")).unwrap(),
                _ => serde_json::from_slice(&response.bytes().await.expect("bytes response")).unwrap(),
            };
            assert_eq!(body, json!({ "created": true }));
        }
        server.abort();
    }
}
