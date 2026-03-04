// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, KnownHost};
use base64::Engine;
use eyre::{Result, eyre};
use reqwest::{Client, Method, multipart};
use std::collections::HashMap;
use url::Url;

/// An exporter that sends requests to an Kibana cluster.
#[derive(Clone)]
pub struct KibanaClient {
    client: Client,
    url: Url,
}

/// A reqwest-based client with authentication for Kibana
impl KibanaClient {
    /// Create a new KibanaExporter from a URL and Auth
    pub fn try_new(url: Url, auth: Auth) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("kbn-xsrf", "true".parse()?);
        match auth {
            Auth::Basic(username, password) => {
                let credentials = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", username, password));
                headers.append(
                    reqwest::header::AUTHORIZATION,
                    format!("Basic {}", credentials).parse()?,
                );
            }
            Auth::Apikey(apikey) => {
                headers.append(
                    reqwest::header::AUTHORIZATION,
                    format!("ApiKey {}", apikey).parse()?,
                );
            }
            Auth::None => {
                headers.append(reqwest::header::AUTHORIZATION, "None".parse()?);
            }
        }
        let client = Client::builder().default_headers(headers).build()?;

        Ok(Self { client, url })
    }

    /// Send a request to a given path on the Kibana client
    pub async fn request(
        &self,
        method: Method,
        headers: &HashMap<String, String>,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::Response> {
        let mut headers: reqwest::header::HeaderMap = headers
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap()))
            .collect();
        let use_form_data = match headers.get("Content-Type") {
            Some(content_type) => {
                log::debug!("Content-Type: {}", content_type.to_str()?);
                content_type.to_str()?.starts_with("multipart/form-data")
            }
            None => false,
        };

        if use_form_data {
            // Reqwest inserts its own multipart Content-Type headers,
            // this removal prevents conflicts
            headers.remove("Content-Type");
        }

        let request = match path.split_once('?') {
            Some((p, query)) => {
                let query: Vec<_> = query.split('&').filter_map(|s| s.split_once('=')).collect();
                self.client
                    .request(method, self.url.join(p)?)
                    .query(&query)
                    .headers(headers)
            }
            None => self
                .client
                .request(method, self.url.join(path)?)
                .headers(headers),
        };

        let response = match body {
            Some(body) if use_form_data => {
                // As of October 2025 we're using a static filename, as the only
                // use of form data is dashboards.ndjson for the saved objects API
                log::debug!("Sending request with form-data");
                let part = multipart::Part::bytes(body.to_vec())
                    .file_name("dashboards.ndjson")
                    .mime_str("application/x-ndjson")?;
                let form = multipart::Form::new().part("file", part);
                request.multipart(form).send().await
            }
            Some(body) => {
                log::debug!("Sending request with body");
                request.body(body.to_vec()).send().await
            }
            None => request.send().await,
        };
        response.map_err(|e| eyre!("Failed to send request: {}", e))
    }

    /// Verify the connection and authentication to Kibana
    pub async fn test_connection(&self) -> Result<reqwest::Response> {
        self.request(Method::GET, &HashMap::new(), "/api/status", None)
            .await
    }
}

impl TryFrom<KnownHost> for KibanaClient {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let auth = host.get_auth();
        KibanaClient::try_new(url, auth)
    }
}

impl std::fmt::Display for KibanaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
