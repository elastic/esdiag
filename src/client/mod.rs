// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Builder for the Elasticsearch client
mod elasticsearch;
/// Client for Kibana APIs
mod kibana;

pub use elasticsearch::{ElasticsearchBuilder, ElasticsearchClient};
pub use kibana::KibanaClient;

extern crate elasticsearch as es;
use crate::data::{Product, Uri};
use eyre::{Result, eyre};
use reqwest::Method;
use std::collections::HashMap;

/// A standardized client for interacting with Elastic Stack APIs
pub enum Client {
    Elasticsearch(ElasticsearchClient),
    Kibana(KibanaClient),
}

impl Client {
    /// Send an HTTP request to a path on the client's base URL
    pub async fn request(
        &self,
        method: Method,
        headers: &HashMap<String, String>,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::Response> {
        log::debug!("Request: {method} {path}");
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
                            log::warn!("Failed to parse header: {:?}", x);
                            None
                        }
                    })
                    .collect();
                use es::http::request::JsonBody;
                let body: Option<JsonBody<serde_json::Value>> = body
                    .map(serde_json::from_slice)
                    .transpose()?
                    .map(JsonBody::new);
                let response = client
                    .send(
                        method,
                        path,
                        header_map,
                        Option::<&serde_json::Value>::None,
                        body,
                        None,
                    )
                    .await?;
                Ok(response.into())
            }
            Client::Kibana(client) => client.request(method, headers, path, body).await,
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
                    .map_err(|e| format!("{e}"))?;

                let status = response.status_code();
                let json: serde_json::Value = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to read test body: {e}"))?;
                log::debug!("Test response {} ", json);

                match json.get("tagline") {
                    Some(_) => Ok(format!("{} ✅ Elasticsearch", status)),
                    None => Err(format!(
                        "{} ❌ No tagline? Host is not an Elasticsearch cluster!",
                        status
                    )),
                }
            }
            Client::Kibana(client) => {
                let response = client.test_connection().await.map_err(|e| format!("{e}"))?;
                let status = response.status();
                let json: serde_json::Value = response
                    .json::<serde_json::Value>()
                    .await
                    .map_err(|e| format!("Failed to read test body: {e}"))?;
                log::debug!("Test response {} ", json);
                match json.get("name") {
                    Some(name) => Ok(format!("{status} ✅ Kibana: {name}")),
                    None => Err(format!("{status} ❌ Host is not an Kibana node!")),
                }
            }
        }
    }

    /// Check if security is enabled on the cluster.
    ///
    /// For Elasticsearch, this checks the `security.enabled` flag in `/_xpack/usage`.
    /// For Kibana, this currently always returns `true`.
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
                            log::debug!(
                                "Security detection returned {status}. Security is enabled but access to /_xpack/usage is restricted."
                            );
                            Ok(true)
                        }
                        404 => {
                            log::debug!(
                                "Security detection returned 404. Assuming security is disabled or not supported."
                            );
                            Ok(false)
                        }
                        _ => {
                            log::warn!("Failed to check security status (HTTP {status}).");
                            Err(eyre!("Failed to check security status: HTTP {status}"))
                        }
                    }
                }
            }
            Client::Kibana(_) => {
                // For Kibana we assume true for now as requested
                Ok(true)
            }
        }
    }
}

impl From<Client> for Product {
    fn from(client: Client) -> Self {
        match client {
            Client::Elasticsearch(_) => Product::Elasticsearch,
            Client::Kibana(_) => Product::Kibana,
        }
    }
}

impl From<&Client> for Product {
    fn from(client: &Client) -> Self {
        match client {
            Client::Elasticsearch(_) => Product::Elasticsearch,
            Client::Kibana(_) => Product::Kibana,
        }
    }
}

impl std::fmt::Display for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Client::Elasticsearch(_) => write!(f, "elasticsearch"),
            Client::Kibana(_) => write!(f, "kibana"),
        }
    }
}

impl TryFrom<Uri> for Client {
    type Error = eyre::Report;

    fn try_from(uri: Uri) -> Result<Self, Self::Error> {
        match uri {
            Uri::KnownHost(host) => match host.app() {
                Product::Kibana => Ok(Client::Kibana(KibanaClient::try_from(host)?)),
                Product::Elasticsearch => {
                    Ok(Client::Elasticsearch(ElasticsearchClient::try_from(host)?))
                }
                _ => Err(eyre!("Unsupported product: {}", host.app())),
            },
            _ => Err(eyre!("Unsupported URI")),
        }
    }
}
