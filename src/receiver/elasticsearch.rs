use super::{Receive, ReceiveRaw};
use crate::{
    client::KnownHost,
    data::{
        diagnostic::{
            DataSource, DiagnosticManifest, data_source::PathType, manifest::ManifestBuilder,
        },
        elasticsearch::Cluster,
    },
};
use elasticsearch::{Elasticsearch, http};
use eyre::Result;
use serde::de::DeserializeOwned;
use url::Url;

#[derive(Clone)]
pub struct ElasticsearchReceiver {
    client: Elasticsearch,
    url: Url,
}

impl ElasticsearchReceiver {
    pub fn new(url: Url) -> Result<Self> {
        let client = Elasticsearch::default();
        Ok(Self { client, url })
    }
}

impl TryFrom<KnownHost> for ElasticsearchReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = Elasticsearch::try_from(host)?;
        Ok(Self { client, url })
    }
}

impl Receive for ElasticsearchReceiver {
    async fn collection_date(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

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
        let path = T::source(PathType::Url)?;
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

        response.json::<T>().await.map_err(Into::into)
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let collection_date = chrono::Utc::now().to_rfc3339();
        log::info!("Creating diagnostic manifest with collection date {collection_date}");
        let cluster = self.get::<Cluster>().await?;
        let manifest = ManifestBuilder::from(cluster)
            .runner("esdiag")
            .collection_date(collection_date)
            .build();
        Ok(manifest.try_into()?)
    }
}

impl ReceiveRaw for ElasticsearchReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        // Get the API URL path for the provided type
        let path = T::source(PathType::Url)?;
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

        response.text().await.map_err(Into::into)
    }
}

impl std::fmt::Display for ElasticsearchReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elasticsearch {}", self.url)
    }
}
