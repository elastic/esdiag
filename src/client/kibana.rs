// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, KnownHost};
use base64::Engine;
use eyre::{Result, eyre};
use reqwest::{Client, Method};
use url::Url;

/// An exporter that sends requests to an Kibana cluster.
#[derive(Clone)]
pub struct KibanaClient {
    client: Client,
    url: Url,
}

impl KibanaClient {
    /// Create a new KibanaExporter from a URL and Auth
    pub fn try_new(url: Url, auth: Auth) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("kbn-xrsf", "true".parse().unwrap());
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        match auth {
            Auth::Basic(username, password) => {
                headers.append(
                    reqwest::header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD
                            .encode(format!("{}:{}", username, password))
                    )
                    .parse()
                    .unwrap(),
                );
            }
            Auth::Apikey(apikey) => {
                headers.append(
                    reqwest::header::AUTHORIZATION,
                    format!("ApiKey {}", apikey).parse().unwrap(),
                );
            }
            Auth::None => {
                headers.append(reqwest::header::AUTHORIZATION, "None".parse().unwrap());
            }
        }
        let client = Client::builder().default_headers(headers).build()?;

        Ok(Self { client, url })
    }

    /// Request to an arbitrary path on the Kibana client
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::Response> {
        let client = self.client.request(method, self.url.join(path)?);
        let response = match body {
            Some(body) => client.body(body.to_vec()).send().await,
            None => client.send().await,
        };
        response.map_err(|e| eyre!("Failed to send request: {}", e))
    }

    pub async fn test_connection(&self) -> Result<reqwest::Response> {
        self.request(Method::GET, "/api/status", None).await
    }
}

impl TryFrom<KnownHost> for KibanaClient {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let auth = host.get_auth();
        Ok(KibanaClient::try_new(url, auth)?)
    }
}

impl std::fmt::Display for KibanaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
