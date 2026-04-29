// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, KnownHost};
use base64::Engine;
use eyre::{Result, eyre};
use reqwest::{Client, Method};
use std::collections::HashMap;
use url::Url;

/// A reqwest-based client with authentication for Logstash
#[derive(Clone)]
pub struct LogstashClient {
    client: Client,
    url: Url,
}

impl LogstashClient {
    /// Create a new Logstash client from a URL and Auth
    pub fn try_new(url: Url, auth: Auth, ignore_certs: bool) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();

        match auth {
            Auth::Basic(username, password) => {
                let credentials =
                    base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
                headers.append(
                    reqwest::header::AUTHORIZATION,
                    format!("Basic {}", credentials).parse()?,
                );
            }
            Auth::Apikey(apikey) => {
                headers.append(reqwest::header::AUTHORIZATION, format!("ApiKey {}", apikey).parse()?);
            }
            Auth::None => {}
        }

        let client = Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(ignore_certs)
            .build()?;

        Ok(Self { client, url })
    }

    /// Send a request to a given path on the Logstash client
    pub async fn request(
        &self,
        method: Method,
        headers: &HashMap<String, String>,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::Response> {
        let headers: reqwest::header::HeaderMap = headers
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
            .collect();

        let request = match path.split_once('?') {
            Some((p, query)) => {
                let query = Self::query_pairs(query);
                self.client
                    .request(method, self.url.join(p)?)
                    .query(&query)
                    .headers(headers)
            }
            None => self.client.request(method, self.url.join(path)?).headers(headers),
        };

        let response = match body {
            Some(body) => request.body(body.to_vec()).send().await,
            None => request.send().await,
        };

        response.map_err(|e| eyre!("Failed to send request: {}", e))
    }

    fn query_pairs(query: &str) -> Vec<(&str, &str)> {
        query
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|s| s.split_once('=').unwrap_or((s, "")))
            .collect()
    }

    /// Verify the connection and authentication to Logstash
    pub async fn test_connection(&self) -> Result<reqwest::Response> {
        self.request(Method::GET, &HashMap::new(), "/", None).await
    }
}

impl TryFrom<KnownHost> for LogstashClient {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url()?;
        let ignore_certs = host.accept_invalid_certs();
        let auth = host.get_auth()?;
        LogstashClient::try_new(url, auth, ignore_certs)
    }
}

impl std::fmt::Display for LogstashClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

#[cfg(test)]
mod tests {
    use super::LogstashClient;

    #[test]
    fn query_pairs_preserve_valueless_parameters() {
        assert_eq!(
            LogstashClient::query_pairs("human&threads=10000"),
            vec![("human", ""), ("threads", "10000")]
        );
    }
}
