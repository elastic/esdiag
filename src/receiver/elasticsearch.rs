// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::processor::{
    DataSource, DiagnosticManifest, ElasticsearchCluster, ManifestBuilder, PathType,
    StreamingDataSource,
};
use super::{Receive, ReceiveRaw};
use crate::data::KnownHost;
use elasticsearch::{Elasticsearch, http};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::process::Command;
use url::Url;

use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone)]
pub struct ElasticsearchReceiver {
    client: Elasticsearch,
    url: Url,
    version: Arc<OnceCell<semver::Version>>,
}

impl ElasticsearchReceiver {
    pub fn new(url: Url) -> Result<Self> {
        let client = Elasticsearch::default();
        Ok(Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
        })
    }

    pub async fn get_version(&self) -> Result<&semver::Version> {
        self.version
            .get_or_try_init(|| async {
                log::debug!("Fetching version from {}", self.url);
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
                    .await?;
                let bytes = response.bytes().await?;
                let cluster: Value = serde_json::from_slice(&bytes)?;
                let version_str = cluster
                    .get("version")
                    .and_then(|v| v.get("number"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| eyre!("No version found in root response"))?;
                semver::Version::parse(version_str)
                    .map_err(|e| eyre!("Failed to parse version: {}", e))
            })
            .await
    }

    pub async fn get_raw_by_path(
        &self,
        path: &str,
        extension: &str,
        path_type: PathType,
    ) -> Result<String> {
        match path_type {
            PathType::Url => {
                log::debug!("Getting raw API path: {}", path);

                let mut headers = http::headers::HeaderMap::new();
                // By default, the Elasticsearch client enforces Accept: application/json
                // We use the configured file extension to request the appropriate content type
                if extension == ".txt" {
                    headers.append(http::headers::ACCEPT, "text/plain".parse().unwrap());
                } else {
                    headers.append(http::headers::ACCEPT, "application/json".parse().unwrap());
                }

                let response = self
                    .client
                    .send(
                        http::Method::Get,
                        path,
                        headers,
                        Option::<&String>::None,
                        Option::<&String>::None,
                        None,
                    )
                    .await?;

                response.text().await.map_err(Into::into)
            }
            PathType::SystemCall => run_system_command(path).await,
            PathType::File => Err(eyre!("PathType::File is not supported for raw-by-path")),
        }
    }
}

async fn run_system_command(command: &str) -> Result<String> {
    let command = command.to_string();
    tokio::task::spawn_blocking(move || {
        let output = if cfg!(windows) {
            Command::new("cmd").args(["/C", &command]).output()
        } else {
            Command::new("sh").args(["-c", &command]).output()
        }
        .map_err(|e| eyre!("failed to execute command '{}': {}", command, e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("command '{}' exited with status {}", command, output.status)
            } else {
                format!(
                    "command '{}' exited with status {}: {}",
                    command, output.status, stderr
                )
            };
            Err(eyre!(msg))
        }
    })
    .await
    .map_err(|e| eyre!("command execution task failed: {}", e))?
}

impl TryFrom<KnownHost> for ElasticsearchReceiver {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        let url = host.get_url();
        let client = Elasticsearch::try_from(host)?;
        Ok(Self {
            client,
            url,
            version: Arc::new(OnceCell::new()),
        })
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

    fn filename(&self) -> Option<String> {
        None
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DataSource + DeserializeOwned,
    {
        // Get the API URL path for the provided type
        let version = self.get_version().await.ok();
        let path = T::source(PathType::Url, version)?;
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

        match response.status_code() {
            http::StatusCode::OK => response.json::<T>().await.map_err(Into::into),
            status => {
                let body: Value = response.json::<Value>().await?;
                log::debug!("Failed to get API response: {}", body);
                let reason = body
                    .get("error")
                    .and_then(|e| e.get("reason").and_then(|r| r.as_str()))
                    .unwrap_or("Unknown");
                Err(eyre!("http {} - {}", status, reason))
            }
        }
    }

    async fn get_stream<T>(&self) -> Result<BoxStream<'static, Result<T::Item>>>
    where
        T: StreamingDataSource + DeserializeOwned,
        T::Item: DeserializeOwned + Send + 'static,
    {
        // TODO: Implement proper streaming from Elasticsearch response body
        // The elasticsearch-rs client currently doesn't easily expose a streaming response body
        // compatible with serde_json::Deserializer.
        Err(eyre!(
            "Streaming is not yet implemented for Elasticsearch receiver"
        ))
    }

    async fn try_get_manifest(&self) -> Result<DiagnosticManifest> {
        let collection_date = chrono::Utc::now().to_rfc3339();
        log::info!("Creating diagnostic manifest with collection date {collection_date}");
        let cluster = match self.get::<ElasticsearchCluster>().await {
            Ok(cluster) => cluster,
            Err(err) => {
                log::debug!("Failed to get Elasticsearch cluster info: {}", err);
                return Err(err);
            }
        };
        let manifest = ManifestBuilder::from(cluster)
            .runner("esdiag")
            .collection_date(collection_date)
            .build();
        Ok(manifest.into())
    }
}

impl ReceiveRaw for ElasticsearchReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        // Get the API URL path for the provided type
        let version = self.get_version().await.ok();
        let path = T::source(PathType::Url, version)?;

        let name = T::name();
        let aliases = T::aliases();
        let source_conf =
            crate::processor::diagnostic::data_source::get_source(T::product(), &name, &aliases)?;
        let extension = source_conf.1.extension.as_deref().unwrap_or(".json");

        self.get_raw_by_path(&path, extension, PathType::Url).await
    }
}

impl std::fmt::Display for ElasticsearchReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Elasticsearch {}", self.url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn raw_by_path_executes_system_call() {
        let receiver =
            ElasticsearchReceiver::new(Url::parse("http://localhost:9200").expect("valid URL"))
                .expect("receiver");

        let command = if cfg!(windows) {
            "echo hello"
        } else {
            "printf hello"
        };

        let output = receiver
            .get_raw_by_path(command, ".txt", PathType::SystemCall)
            .await
            .expect("system call should succeed");

        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn raw_by_path_system_call_propagates_command_errors() {
        let receiver =
            ElasticsearchReceiver::new(Url::parse("http://localhost:9200").expect("valid URL"))
                .expect("receiver");

        let err = receiver
            .get_raw_by_path(
                "definitely_not_a_real_command_987654",
                ".txt",
                PathType::SystemCall,
            )
            .await
            .expect_err("invalid command should fail");

        let msg = err.to_string();
        assert!(
            msg.contains("command") || msg.contains("failed to execute"),
            "unexpected error message: {msg}"
        );
    }

    #[tokio::test]
    async fn raw_by_path_rejects_file_path_type() {
        let receiver =
            ElasticsearchReceiver::new(Url::parse("http://localhost:9200").expect("valid URL"))
                .expect("receiver");

        let err = receiver
            .get_raw_by_path("ignored", ".txt", PathType::File)
            .await
            .expect_err("file path type should be rejected");

        assert!(err.to_string().contains("PathType::File"));
    }
}
