use eyre::{Result, eyre};
use url::Url;

use crate::receiver::archive::ArchiveBytesReceiver;

#[derive(Clone)]
pub struct ElasticUploadDownloader {
    token: String,
    url: Url,
}

impl ElasticUploadDownloader {
    /// Downloads a file from the Elastic Uploader service given a URL and token
    /// The URL format of `https://upload.elastic.co/...` will have been validated previously.
    pub fn download(self) -> Result<ArchiveBytesReceiver> {
        // Using block_in_place allows a synchronous file download inside an async runtime
        tokio::task::block_in_place(|| {
            let client = reqwest::blocking::Client::new();
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "Authorization",
                reqwest::header::HeaderValue::from_str(&self.token)?,
            );
            let request = client.get(self.url.clone()).headers(headers);
            let response = request.send()?;
            let bytes = response.bytes()?;
            log::debug!("Downloaded archive size: {} bytes", bytes.len());
            Ok(ArchiveBytesReceiver::try_from(bytes)?)
        })
    }
}

impl TryFrom<Url> for ElasticUploadDownloader {
    type Error = eyre::Report;

    fn try_from(url: Url) -> Result<Self> {
        let mut url = url.clone();
        let token = url
            .password()
            .ok_or_else(|| eyre!("No token provided"))?
            .to_string();
        // Since token authentication is by header, clear provided username and password from the URL
        url.set_username("").ok();
        url.set_password(None).ok();
        log::info!("Downloading archive from {url}");
        Ok(Self { token, url })
    }
}

impl std::fmt::Display for ElasticUploadDownloader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elastic Uploader {}", self.url)
    }
}
