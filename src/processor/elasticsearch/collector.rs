// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{
    collector::{CollectOptions, CollectionResult},
    diagnostic::PathType,
};
use super::{
    AliasList, Cluster, ClusterSettings, DataSource, DataStreams, DiagnosticManifest, HealthReport,
    IlmExplain, IlmPolicies, IndicesSettings, IndicesStats, Licenses, MappingStats, Nodes,
    NodesStats, PendingTasks, SearchableSnapshotsCacheStats, SearchableSnapshotsStats, SlmPolicies,
    Tasks,
};
use crate::{data::Product, exporter::DirectoryExporter, receiver::Receiver};
use eyre::Result;
use std::path::PathBuf;

use crate::processor::api::{ApiResolver, ApiWeight, DiagnosticType, ElasticsearchApi};
use futures::stream::{self, StreamExt};
use std::str::FromStr;
use std::time::Duration;

pub struct ElasticsearchCollector {
    receiver: Receiver,
    exporter: DirectoryExporter,
    options: CollectOptions,
}

impl ElasticsearchCollector {
    pub async fn new(
        receiver: Receiver,
        exporter: DirectoryExporter,
        options: CollectOptions,
    ) -> Result<Self> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let directory = format!("api-diagnostics-{}", timestamp);
        Ok(Self {
            receiver,
            exporter: exporter.collection_directory(directory)?,
            options,
        })
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let diag_type = DiagnosticType::from_str(&self.options.r#type)?;
        let apis = ApiResolver::resolve_es(
            &diag_type,
            self.options.include.as_ref(),
            self.options.exclude.as_ref(),
        )?;

        let api_names: Vec<&str> = apis.iter().map(|a| a.as_str()).collect();
        log::debug!("Resolved APIs for collection: {:?}", api_names);

        let mut result = CollectionResult {
            path: self.exporter.to_string().clone(),
            success: 0,
            total: apis.len() + 1, // +1 for manifest
        };

        let mut collected_api_names = Vec::new();

        result.success += self.save_diagnostic_manifest(&apis).await?;

        let mut heavy_apis = Vec::new();
        let mut light_apis = Vec::new();

        for api in apis {
            collected_api_names.push(api.as_str().to_string());
            if api.weight() == ApiWeight::Heavy {
                heavy_apis.push(api);
            } else {
                light_apis.push(api);
            }
        }

        // Concurrent fetch for Light APIs
        let mut light_stream = stream::iter(light_apis)
            .map(|api| async move { self.save_api_with_retry(&api).await })
            .buffer_unordered(5);

        while let Some(res) = light_stream.next().await {
            result.success += res;
        }

        // Sequential fetch for Heavy APIs
        for api in heavy_apis {
            result.success += self.save_api_with_retry(&api).await;
        }

        Ok(result)
    }

    async fn save_api_with_retry(&self, api: &ElasticsearchApi) -> usize {
        let max_duration = Duration::from_secs(300); // 5 minutes
        let start_time = std::time::Instant::now();
        let mut attempt = 1;
        let mut delay = Duration::from_secs(2);

        loop {
            match self.save_api(api).await {
                Ok(success) => return success,
                Err(e) => {
                    if start_time.elapsed() > max_duration {
                        log::error!(
                            "Failed to save {} after {} attempts (5 min timeout): {}",
                            api.as_str(),
                            attempt,
                            e
                        );
                        return 0;
                    }
                    log::warn!(
                        "Attempt {} failed for {}: {}. Retrying in {:?}...",
                        attempt,
                        api.as_str(),
                        e,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(60));
                }
            }
        }
    }

    async fn save_api(&self, api: &ElasticsearchApi) -> Result<usize> {
        match api {
            ElasticsearchApi::AliasList => self.save::<AliasList>().await,
            ElasticsearchApi::Cluster => self.save::<Cluster>().await,
            ElasticsearchApi::ClusterSettings => self.save::<ClusterSettings>().await,
            ElasticsearchApi::DataStreams => self.save::<DataStreams>().await,
            ElasticsearchApi::HealthReport => self.save::<HealthReport>().await,
            ElasticsearchApi::IlmExplain => self.save::<IlmExplain>().await,
            ElasticsearchApi::IlmPolicies => self.save::<IlmPolicies>().await,
            ElasticsearchApi::IndicesSettings => self.save::<IndicesSettings>().await,
            ElasticsearchApi::IndicesStats => self.save::<IndicesStats>().await,
            ElasticsearchApi::Licenses => self.save::<Licenses>().await,
            ElasticsearchApi::MappingStats => self.save::<MappingStats>().await,
            ElasticsearchApi::Nodes => self.save::<Nodes>().await,
            ElasticsearchApi::NodesStats => self.save::<NodesStats>().await,
            ElasticsearchApi::PendingTasks => self.save::<PendingTasks>().await,
            ElasticsearchApi::SearchableSnapshotsCacheStats => {
                self.save::<SearchableSnapshotsCacheStats>().await
            }
            ElasticsearchApi::SearchableSnapshotsStats => {
                self.save::<SearchableSnapshotsStats>().await
            }
            ElasticsearchApi::SlmPolicies => self.save::<SlmPolicies>().await,
            ElasticsearchApi::Tasks => self.save::<Tasks>().await,
        }
    }

    async fn save<T>(&self) -> Result<usize>
    where
        T: DataSource,
    {
        let content = match self.receiver.get_raw::<T>().await {
            Ok(c) => c,
            Err(e) => {
                if let Some(ds_err) = e.downcast_ref::<crate::processor::diagnostic::data_source::DataSourceError>() {
                    log::debug!("Skipping {} collection: {}", T::name(), ds_err);
                    return Ok(0);
                }
                return Err(e);
            }
        };
        let path = PathBuf::from(T::source(PathType::File, None)?);
        let filename = format!("{}", path.display());
        match self.exporter.save(path, content).await {
            Ok(()) => {
                log::info!("Saved {filename}");
                Ok(1)
            }
            Err(e) => {
                log::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save_diagnostic_manifest(&self, apis: &Vec<ElasticsearchApi>) -> Result<usize> {
        let cluster = self.receiver.get::<Cluster>().await?;
        let collected_api_names: Vec<String> =
            apis.iter().map(|a| a.as_str().to_string()).collect();

        let manifest = DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some(self.options.r#type.clone()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(cluster.version.number.to_string()),
        )
        .with_identifiers(self.options.identifiers.clone())
        .with_collected_apis(collected_api_names);

        let path = PathBuf::from(DiagnosticManifest::source(PathType::File, None)?);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        log::info!("Saved {filename}");
        Ok(1)
    }
}
