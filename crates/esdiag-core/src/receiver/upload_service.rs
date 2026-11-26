// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{Result, eyre};
use url::Url;

use crate::receiver::archive::ArchiveBytesReceiver;
use bytes::Bytes;
use std::time::Duration;

/// Bounds a whole download, body included, so a stalled Upload Service
/// connection fails instead of holding its caller indefinitely.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

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
        let client = reqwest::Client::builder().timeout(DOWNLOAD_TIMEOUT).build()?;
        let response = client.get(self.url).headers(headers).send().await?;
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
        // `Url` keeps userinfo percent-encoded; the header carries the token itself.
        let encoded = url.password().ok_or_else(|| eyre!("No token provided"))?;
        let token = urlencoding::decode(encoded)
            .map_err(|_| eyre!("Download token is not valid UTF-8"))?
            .into_owned();
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

#[cfg(test)]
mod tests {
    use super::UploadServiceDownloader;

    #[test]
    fn the_header_token_is_the_decoded_url_password() {
        let token = "a/b:c=d@e%41f+g%";
        let mut url = url::Url::parse("https://upload.elastic.co/d/abc").unwrap();
        url.set_username("token").unwrap();
        crate::data::set_upload_token(&mut url, token).unwrap();
        let downloader = UploadServiceDownloader::try_from(url).expect("downloader");
        assert_eq!(downloader.token, token);
        assert_eq!(downloader.url.as_str(), "https://upload.elastic.co/d/abc");
    }
}
