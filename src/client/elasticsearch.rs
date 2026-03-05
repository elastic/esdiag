// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, KnownHost};
use base64::{Engine, engine::general_purpose::STANDARD};
use elasticsearch::{
    cert::CertificateValidation,
    http::{
        headers,
        transport::{SingleNodeConnectionPool, TransportBuilder},
    },
};
use eyre::Result;
use url::Url;

pub use elasticsearch::Elasticsearch as ElasticsearchClient;

pub struct ElasticsearchBuilder {
    cert_validation: CertificateValidation,
    connection_pool: SingleNodeConnectionPool,
    headers: headers::HeaderMap,
}

/// A builder for the official Elasticsearch client
impl ElasticsearchBuilder {
    pub fn new(url: Url) -> Self {
        let mut headers = headers::HeaderMap::new();
        headers.append(headers::ACCEPT_ENCODING, "gzip".parse().unwrap());

        Self {
            cert_validation: CertificateValidation::Default,
            connection_pool: SingleNodeConnectionPool::new(url),
            headers,
        }
    }

    pub fn apikey(self, apikey: String) -> Self {
        let mut headers = self.headers;
        headers.append(
            headers::AUTHORIZATION,
            format!("ApiKey {}", apikey)
                .parse()
                .expect("Invalid API key"),
        );
        Self { headers, ..self }
    }

    pub fn auth(self, auth: Auth) -> Self {
        log::debug!("Setting client auth to {}", auth);
        match auth {
            Auth::Apikey(apikey) => self.apikey(apikey),
            Auth::Basic(username, password) => self.basic_auth(username, password),
            Auth::None => self,
        }
    }

    pub fn basic_auth(self, username: String, password: String) -> Self {
        let mut headers = self.headers;
        headers.append(
            headers::AUTHORIZATION,
            headers::HeaderValue::from_str(&format!(
                "Basic {}",
                STANDARD.encode(format!("{}:{}", username, password))
            ))
            .expect("Invalid basic auth"),
        );
        Self { headers, ..self }
    }

    pub fn build(self) -> Result<ElasticsearchClient> {
        let transport = TransportBuilder::new(self.connection_pool)
            .headers(self.headers)
            .cert_validation(self.cert_validation)
            .request_body_compression(true)
            .build()?;
        Ok(ElasticsearchClient::new(transport))
    }

    pub fn insecure(self, ignore_certs: bool) -> Self {
        let cert_validation = match ignore_certs {
            true => CertificateValidation::None,
            false => CertificateValidation::Default,
        };
        Self {
            cert_validation,
            ..self
        }
    }
}

impl TryFrom<KnownHost> for ElasticsearchClient {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<ElasticsearchClient> {
        let url = host.get_url();
        let ignore_certs = host.accept_invalid_certs();
        let auth = host.get_auth()?;
        let client = ElasticsearchBuilder::new(url)
            .auth(auth)
            .insecure(ignore_certs)
            .build()?;
        Ok(client)
    }
}
