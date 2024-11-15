use super::Receive;
use crate::client::Host;
use crate::data::{diagnostic::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use elasticsearch::{http, Elasticsearch};
use serde::de::DeserializeOwned;
use url::Url;

#[derive(Clone)]
pub struct ElasticsearchReceiver {
    client: Elasticsearch,
    url: Url,
    uri: Uri,
}

impl ElasticsearchReceiver {
    pub fn new(url: Url, uri: Uri) -> Result<Self> {
        let client = Elasticsearch::default();
        Ok(Self { client, url, uri })
    }
}

impl TryFrom<Uri> for ElasticsearchReceiver {
    type Error = color_eyre::eyre::Report;

    #[allow(unreachable_code, unused_variables)]
    fn try_from(uri: Uri) -> Result<Self> {
        match uri {
            Uri::Host(host) => {
                let (client, url) = match host {
                    Host::ApiKey {
                        accept_invalid_certs,
                        apikey,
                        app,
                        url,
                        ..
                    } => {
                        unimplemented!("ElasticsearchReceiver ApiKey")
                    }
                    Host::Basic {
                        accept_invalid_certs,
                        app,
                        password,
                        url,
                        username,
                        ..
                    } => {
                        unimplemented!("ElasticsearchReceiver Basic")
                    }
                    Host::None { app, url } => {
                        unimplemented!("ElasticsearchReceiver None")
                    }
                };
                Ok(Self { client, url, uri })
            }
            _ => Err(eyre!("Input host was not an Elasticsearch cluster")),
        }
    }
}

impl Receive for ElasticsearchReceiver {
    async fn is_connected(&self) -> bool {
        log::debug!("Testing Elasticsearch client connection");
        // An empty request to `/`
        let response = self
            .client
            .send(
                http::Method::Get,
                "",
                http::headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await;

        match response {
            Ok(response) => {
                log::debug!(
                    "Elasticsearch client connection successful: {}",
                    response.status_code()
                );
                true
            }
            Err(e) => {
                log::error!("Elasticsearch client connection failed: {e}");
                false
            }
        }
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        // Get the API URL path for the provided type
        let path = T::source(&self.uri)?;
        log::debug!("Getting API: {}", &path);

        // Send a simple GET request to the API path
        let response = self
            .client
            .send(
                http::Method::Get,
                &path,
                http::headers::HeaderMap::new(),
                Option::<&String>::None,
                Option::<&String>::None,
                None,
            )
            .await?;

        // turbo-fish serde deserialization of the JSON response
        response.json::<T>().await.map_err(Into::into)
    }

    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        Err(eyre!(
            "ElasticsearchReceiver does not support setting a working directory: {work_dir}"
        ))
    }
}

impl std::fmt::Display for ElasticsearchReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
