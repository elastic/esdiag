// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// The `_alias` API
mod alias;
/// The `_cluster/settings` API
mod cluster_settings;
/// Collector definition for Elasticsearch diagnostics
mod collector;
/// The `_data_stream` API
mod data_stream;
/// The `_health_report` API
mod health_report;
/// The `_ilm/explain` API
mod ilm_explain;
/// The `_ilm/policy` API
mod ilm_policies;
/// The `_settings` API
mod indices_settings;
/// The `_stats` API
mod indices_stats;
/// The `_license` API
mod licenses;
/// The `_mapping` API
mod mapping_stats;
/// Elasticsearch diagnostics metadata
mod metadata;
/// The `_nodes` API
mod nodes;
/// The `_nodes/stats` API
mod nodes_stats;
/// The `_pending_tasks` API
mod pending_tasks;
/// The `_searchable_snapshots_cache/stats` API
mod searchable_snapshots_cache_stats;
/// The `_searchable_snapshots/stats` API
mod searchable_snapshots_stats;
/// The `_slm/policy` API
mod slm_policies;
/// The `_snapshot` API
mod snapshots;
/// The `_tasks` API
mod tasks;
/// The cluster `/` API -- "You know, for search!"
mod version;

use crate::processor::{StreamingDataSource, StreamingDocumentExporter};
pub use collector::ElasticsearchCollector;
pub use metadata::ElasticsearchMetadata;
use tokio::sync::mpsc;
pub use {
    licenses::License,
    version::{Cluster, ClusterMetadata, Version},
};

use super::{
    DataSource, DiagnosticManifest, DiagnosticProcessor, DiagnosticReport, DocumentExporter,
    Metadata, ProcessorSummary,
    api::ProcessSelection,
    diagnostic::{DiagnosticReportBuilder, Lookup},
    elasticsearch::health_report::HealthReport,
};
use crate::{
    data::{self, Product},
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashSet, sync::Arc};
use {
    alias::{Alias, AliasList},
    cluster_settings::{ClusterSettings, ClusterSettingsDefaults},
    data_stream::{DataStreamDocument, DataStreams},
    ilm_explain::{IlmExplain, IlmStats},
    ilm_policies::IlmPolicies,
    indices_settings::{IndexSettings, IndicesSettings},
    indices_stats::IndicesStats,
    licenses::Licenses,
    mapping_stats::{MappingStats, MappingSummary},
    nodes::{NodeDocument, Nodes},
    nodes_stats::NodesStats,
    pending_tasks::PendingTasks,
    searchable_snapshots_cache_stats::{SearchableSnapshotsCacheStats, SharedCacheStats},
    searchable_snapshots_stats::SearchableSnapshotsStats,
    slm_policies::SlmPolicies,
    snapshots::{Repositories, Snapshots},
    tasks::Tasks,
};

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Lookups,
    metadata: ElasticsearchMetadata,
    selected_processors: Option<HashSet<String>>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl ElasticsearchDiagnostic {
    fn should_process(&self, key: &str) -> bool {
        self.selected_processors
            .as_ref()
            .is_none_or(|selected| selected.contains(key))
    }

    async fn process_cluster_settings(
        &self,
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()> {
        let summary = match self.receiver.get::<ClusterSettingsDefaults>().await {
            Ok(settings) => settings
                .documents_export(&self.exporter, &self.lookups, &self.metadata)
                .await
                .was_parsed(),
            Err(defaults_err) => {
                tracing::debug!(
                    "Failed to read cluster_settings_defaults, falling back to cluster_settings: {}",
                    defaults_err
                );
                match self.receiver.get::<ClusterSettings>().await {
                    Ok(settings) => settings
                        .documents_export(&self.exporter, &self.lookups, &self.metadata)
                        .await
                        .was_parsed(),
                    Err(settings_err) => {
                        tracing::warn!(
                            "Failed to read cluster_settings_defaults and cluster_settings: {}; {}",
                            defaults_err,
                            settings_err
                        );
                        ProcessorSummary::new(ClusterSettings::name())
                    }
                }
            }
        };

        summary_tx.send(summary).await.map_err(|err| {
            tracing::error!("Failed to send summary: {}", err);
            eyre!(err)
        })
    }

    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
    {
        match self.receiver.get::<T>().await {
            Ok(data) => {
                let summary = data
                    .documents_export(&self.exporter, &self.lookups, &self.metadata)
                    .await
                    .was_parsed();
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
            Err(err) => {
                tracing::warn!("{}", err);
                let summary = ProcessorSummary::new(T::name());
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
        }
    }

    async fn process_streaming_datasource<T>(
        &self,
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()>
    where
        T: DataSource
            + StreamingDataSource
            + StreamingDocumentExporter<Lookups, ElasticsearchMetadata>
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
        T::Item: DeserializeOwned + Send + 'static,
    {
        match self.receiver.get_stream::<T>().await {
            Ok(stream) => {
                let summary = T::documents_export_stream(
                    stream,
                    &self.exporter,
                    &self.lookups,
                    &self.metadata,
                )
                .await
                .was_parsed();
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
            Err(e) => {
                tracing::debug!(
                    "Streaming failed/not supported for {}, falling back to full load: {}",
                    T::name(),
                    e
                );
                self.process_datasource::<T>(summary_tx).await
            }
        }
    }
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let cluster = receiver.get::<version::Cluster>().await?;
        let display_name = match receiver.get::<ClusterSettingsDefaults>().await {
            Ok(settings) => settings.get_display_name(),
            Err(err) => {
                tracing::debug!(
                    "Failed to read cluster_settings_defaults for display name, falling back to cluster_settings: {}",
                    err
                );
                receiver.get::<ClusterSettings>().await?.get_display_name()
            }
        };
        let metadata =
            ElasticsearchMetadata::try_new(manifest, cluster.with_display_name(display_name))?;

        let mut report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .cluster(metadata.cluster.clone())
            .product(Product::Elasticsearch)
            .receiver(receiver.to_string())
            .build()?;

        let lookups = Lookups {
            alias: Lookup::from(receiver.get::<AliasList>().await),
            data_stream: Lookup::from(receiver.get::<DataStreams>().await),
            index_settings: Lookup::from(receiver.get::<IndicesSettings>().await),
            node: Lookup::from(receiver.get::<Nodes>().await),
            ilm_explain: Lookup::from(receiver.get::<IlmExplain>().await),
            shared_cache: Lookup::from(receiver.get::<SearchableSnapshotsCacheStats>().await),
            mapping_stats: match receiver.get_stream::<MappingStats>().await {
                Ok(stream) => Lookup::<MappingSummary>::from_stream(stream).await,
                Err(e) => {
                    tracing::debug!(
                        "Streaming mappings failed: {}, falling back to full load",
                        e
                    );
                    Lookup::from(receiver.get::<MappingStats>().await)
                }
            },
        };
        let license = receiver
            .get::<Licenses>()
            .await
            .map(|licenses| licenses.license)
            .ok();

        report.add_license(license);
        report.add_lookup("alias", &lookups.alias);
        report.add_lookup("data_stream", &lookups.data_stream);
        report.add_lookup("index_settings", &lookups.index_settings);
        report.add_lookup("node", &lookups.node);
        report.add_lookup("ilm_explain", &lookups.ilm_explain);
        report.add_lookup("shared_cache", &lookups.shared_cache);
        report.add_lookup("mapping_stats", &lookups.mapping_stats);

        Ok((
            Box::new(Self {
                exporter,
                lookups,
                metadata,
                receiver,
                selected_processors: process_selection
                    .map(|selection| selection.selected.into_iter().collect()),
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        tracing::debug!("Running Elasticsearch diagnostic processors");
        if !self.exporter.is_connected().await {
            return Err(eyre!("Exporter is not connected"));
        }

        if tracing::enabled!(tracing::Level::DEBUG) {
            data::save_file("diagnostic.json", &self)?;
        }

        let diag = Arc::new(self);
        // Future 1: IndicesStats
        let (diag_idx, summary_tx_idx) = (diag.clone(), summary_tx.clone());
        let thread1 = async move {
            if diag_idx.should_process("indices_stats") {
                diag_idx
                    .process_streaming_datasource::<IndicesStats>(summary_tx_idx)
                    .await?;
            }
            Ok::<(), eyre::Error>(())
        };

        // Future 2: NodesStats
        let (diag_nodes, summary_tx_nodes) = (diag.clone(), summary_tx.clone());
        let thread2 = async move {
            if diag_nodes.should_process("nodes_stats") {
                diag_nodes
                    .process_streaming_datasource::<NodesStats>(summary_tx_nodes)
                    .await?;
            }
            Ok::<(), eyre::Error>(())
        };

        // Future 3: Everything else
        let thread3 = async move {
            if diag.should_process("cluster_settings")
                || diag.should_process("cluster_settings_defaults")
            {
                diag.process_cluster_settings(summary_tx.clone()).await?;
            }
            if diag.should_process("health_report") {
                diag.process_datasource::<HealthReport>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("ilm_policies") {
                diag.process_datasource::<IlmPolicies>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("indices_settings") {
                diag.process_datasource::<IndicesSettings>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("nodes") {
                diag.process_datasource::<Nodes>(summary_tx.clone()).await?;
            }
            if diag.should_process("pending_tasks") {
                diag.process_datasource::<PendingTasks>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("slm_policies") {
                diag.process_datasource::<SlmPolicies>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("repositories") {
                diag.process_datasource::<Repositories>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("snapshot") {
                diag.process_streaming_datasource::<Snapshots>(summary_tx.clone())
                    .await?;
            }
            if diag.should_process("tasks") {
                diag.process_datasource::<Tasks>(summary_tx.clone()).await?;
            }
            Ok::<(), eyre::Error>(())
        };

        let _ = tokio::try_join!(thread1, thread2, thread3)?;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            self.metadata.cluster.display_name.clone(),
            self.metadata.cluster.uuid.clone(),
            "cluster".to_string(),
        )
    }
}

impl ElasticsearchDiagnostic {
    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }
}

#[derive(Clone, Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStreamDocument>,
    pub ilm_explain: Lookup<IlmStats>,
    pub index_settings: Lookup<IndexSettings>,
    pub mapping_stats: Lookup<MappingSummary>,
    pub node: Lookup<NodeDocument>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
