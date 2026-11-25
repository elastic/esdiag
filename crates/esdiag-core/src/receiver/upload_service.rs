// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{Result, eyre};
use url::Url;

use crate::receiver::archive::ArchiveBytesReceiver;
use bytes::Bytes;

#[derive(Clone)]
pub struct UploadServiceDownloader {
    token: String,
    url: Url,
}

impl UploadServiceDownloader {
    /// Downloads a file from the Elastic Upload Service given a URL and token
    /// The URL format of `https://upload.elastic.co/...` will have been validated previously.
    pub async fn download(self) -> Result<ArchiveBytesReceiver> {
        ArchiveBytesReceiver::try_from(self.download_bytes().await?)
    }

    /// Dropping the returned future closes the download connection.
    pub async fn download_bytes(self) -> Result<Bytes> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Authorization", reqwest::header::HeaderValue::from_str(&self.token)?);
        let response = reqwest::Client::new().get(self.url).headers(headers).send().await?;
        let bytes = response.bytes().await?;
        tracing::debug!("Downloaded archive size: {} bytes", bytes.len());
        match bytes.len() {
            0 => Err(eyre!("Downloaded empty file, check upload link expiration")),
            _ => Ok(bytes),
        }
    }
}

impl TryFrom<Url> for UploadServiceDownloader {
    type Error = eyre::Report;

    fn try_from(url: Url) -> Result<Self> {
        let mut url = url.clone();
        let token = url.password().ok_or_else(|| eyre!("No token provided"))?.to_string();
        // Since token authentication is by header, clear provided username and password from the URL
        url.set_username("").ok();
        url.set_password(None).ok();
        tracing::info!("Downloading archive from {url}");
        Ok(Self { token, url })
    }
}

impl std::fmt::Display for UploadServiceDownloader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elastic Upload Service {}", self.url)
    }
}
