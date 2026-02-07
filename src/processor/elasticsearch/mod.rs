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
/// The `_mapping` API
mod mapping_stats;
/// The `_license` API
mod licenses;
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
/// The `_tasks` API
mod tasks;
/// The cluster `/` API -- "You know, for search!"
mod version;

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
use std::sync::Arc;
use {
    alias::{Alias, AliasList},
    cluster_settings::ClusterSettings,
    data_stream::{DataStreamDocument, DataStreams},
    ilm_explain::{IlmExplain, IlmStats},
    ilm_policies::IlmPolicies,
    indices_settings::{IndexSettings, IndicesSettings},
    indices_stats::IndicesStats,
    mapping_stats::{MappingStats, MappingSummary},
    licenses::Licenses,
    nodes::{NodeDocument, Nodes},
    nodes_stats::NodesStats,
    pending_tasks::PendingTasks,
    searchable_snapshots_cache_stats::{SearchableSnapshotsCacheStats, SharedCacheStats},
    searchable_snapshots_stats::SearchableSnapshotsStats,
    slm_policies::SlmPolicies,
    tasks::Tasks,
};

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Lookups,
    metadata: ElasticsearchMetadata,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl ElasticsearchDiagnostic {
    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
    {
        let summary = match self.receiver.get::<T>().await {
            Ok(data) => data
                .documents_export(&self.exporter, &self.lookups, &self.metadata)
                .await
                .was_parsed(),
            Err(err) => {
                log::warn!("{}", err);
                ProcessorSummary::new(T::name())
            }
        };
        summary_tx.send(summary).await.map_err(|err| {
            log::error!("Failed to send summary: {}", err);
            eyre!(err)
        })
    }
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let cluster = receiver.get::<version::Cluster>().await?;
        let display_name = receiver
            .get::<cluster_settings::ClusterSettings>()
            .await?
            .get_display_name();
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
            mapping_stats: Lookup::from(receiver.get::<MappingStats>().await),
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
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        log::debug!("Running Elasticsearch diagnostic processors");
        if self.exporter.is_connected().await == false {
            return Err(eyre!("Exporter is not connected"));
        }

        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let diag = Arc::new(self);
        // Thread 1: IndicesStats
        let (diag_idx, summary_tx_idx) = (diag.clone(), summary_tx.clone());
        let thread1 = tokio::spawn(async move {
            diag_idx
                .process_datasource::<IndicesStats>(summary_tx_idx)
                .await?;
            Ok::<(), eyre::Error>(())
        });

        // Thread 2: NodesStats
        let (diag_nodes, summary_tx_nodes) = (diag.clone(), summary_tx.clone());
        let thread2 = tokio::spawn(async move {
            diag_nodes
                .process_datasource::<NodesStats>(summary_tx_nodes)
                .await?;
            Ok::<(), eyre::Error>(())
        });

        // Thread 3: Everything else
        let thread3 = tokio::spawn(async move {
            diag.process_datasource::<ClusterSettings>(summary_tx.clone())
                .await?;
            diag.process_datasource::<HealthReport>(summary_tx.clone())
                .await?;
            diag.process_datasource::<IlmPolicies>(summary_tx.clone())
                .await?;
            diag.process_datasource::<IndicesSettings>(summary_tx.clone())
                .await?;
            diag.process_datasource::<Nodes>(summary_tx.clone()).await?;
            diag.process_datasource::<PendingTasks>(summary_tx.clone())
                .await?;
            diag.process_datasource::<SlmPolicies>(summary_tx.clone())
                .await?;
            diag.process_datasource::<Tasks>(summary_tx.clone()).await?;
            Ok::<(), eyre::Error>(())
        });

        match tokio::try_join!(thread1, thread2, thread3) {
            Ok(_) => Ok(()),
            Err(err) => Err(eyre!(err)),
        }
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

#[derive(Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStreamDocument>,
    pub ilm_explain: Lookup<IlmStats>,
    pub index_settings: Lookup<IndexSettings>,
    pub mapping_stats: Lookup<MappingSummary>,
    pub node: Lookup<NodeDocument>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
