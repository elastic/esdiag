// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{
    DataSource, DiagnosticManifest, ElasticsearchCluster, ManifestBuilder, PathType,
};
use super::{Receive, ReceiveRaw};
use crate::data::KnownHost;
use eyre::Result;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION};
use reqwest::{Client, ClientBuilder, header::HeaderMap};
use serde::de::DeserializeOwned;
use url::Url;

#[derive(Clone)]
pub struct ElasticCloudAdminReceiver {
    client: Client,
    url: Url,
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
            .build()?;
        Ok(Self { client, url })
    }
}

impl TryFrom<KnownHost> for ElasticCloudAdminReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        match host {
            KnownHost::ApiKey { apikey, url, .. } => {
                Ok(ElasticCloudAdminReceiver::new(url, apikey)?)
            }
            _ => Err(eyre::eyre!("Elastic Cloud Admin requires a URL and ApiKey")),
        }
    }
}

impl Receive for ElasticCloudAdminReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        log::debug!(
            "Testing Elastic Cloud Admin connection to {}",
            self.url.as_str()
        );
        // An empty request to `/`
        let response = self.client.get(self.url.as_str()).send().await;

        match response {
            Ok(response) => {
                log::debug!("Elastic Cloud connection successful: {}", response.status());
                true
            }
            Err(e) => {
                log::error!("Elastic Cloud connection failed: {e}");
                false
            }
        }
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        // Get the API URL path for the provided type
        let path = T::source(PathType::Url)?;
        // Prepend the API proxy base path
        let path = match path {
            "/" => &format!("{}/", self.url.path()),
            _ => &format!("{}/{}", self.url.path(), path),
        };
        let url = self.url.join(path)?;
        log::debug!("Getting API: {}", url);
        let response = self.client.get(url).send().await?;

        log::debug!("Get Response: {:?}", response);

        response.json::<T>().await.map_err(Into::into)
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let collection_date = chrono::Utc::now().to_rfc3339();
        log::info!("Creating diagnostic manifest with collection date {collection_date}");
        let cluster = self.get::<ElasticsearchCluster>().await?;
        let manifest = ManifestBuilder::from(cluster)
            .runner("esdiag")
            .collection_date(collection_date)
            .build();
        Ok(manifest.try_into()?)
    }
}

impl ReceiveRaw for ElasticCloudAdminReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        // Get the API URL path for the provided type
        let path = T::source(PathType::Url)?;
        // Prepend the API proxy base path
        let path = match path {
            "/" => &format!("{}/", self.url.path()),
            _ => &format!("{}/{}", self.url.path(), path),
        };
        let url = self.url.join(&path)?;
        log::debug!("Getting API: {}", url);
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
