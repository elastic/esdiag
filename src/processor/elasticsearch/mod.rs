// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// The `_alias` API
mod alias;
/// The `_cluster/settings` API
mod cluster_settings;
/// Collector definition for Elasticsearch diagnostics
pub mod collector;
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

pub use metadata::{ElasticsearchMetadata, ElasticsearchVersion};
pub use {
    licenses::License,
    version::{Cluster, Version},
};

use super::{
    DiagnosticProcessor, DocumentExporter, Metadata,
    diagnostic::{
        DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder, Lookup, Product,
    },
};
use crate::{
    data,
    exporter::Exporter,
    processor::{ProcessorSummary, elasticsearch::health_report::HealthReport},
    receiver::Receiver,
};
use eyre::{Result, eyre};
use serde::{Serialize, de::DeserializeOwned};
use {
    alias::{Alias, AliasList},
    cluster_settings::ClusterSettings,
    data_stream::{DataStream, DataStreams},
    ilm_explain::{IlmExplain, IlmStats},
    ilm_policies::IlmPolicies,
    indices_settings::{IndexSettings, IndicesSettings},
    indices_stats::IndicesStats,
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
    exporter: Exporter,
    #[serde(skip)]
    receiver: Receiver,
    #[serde(skip)]
    report: DiagnosticReport,
}

impl ElasticsearchDiagnostic {
    async fn process<T>(&self) -> Result<ProcessorSummary>
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
        Ok(summary)
    }
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
        let cluster = receiver.get::<version::Cluster>().await?;
        let display_name = receiver
            .get::<cluster_settings::ClusterSettings>()
            .await?
            .get_display_name();
        let metadata =
            ElasticsearchMetadata::try_new(manifest, cluster.with_display_name(display_name))?;
        let mut report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
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

        Ok(Box::new(Self {
            exporter,
            lookups,
            metadata,
            receiver,
            report,
        }))
    }

    async fn run(mut self) -> Result<DiagnosticReport> {
        log::debug!("Running Elasticsearch diagnostic processors");
        if self.exporter.is_connected().await == false {
            return Err(eyre!("Exporter is not connected"));
        }

        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        self.report.add_identifiers(self.exporter.identifiers());

        let (thread1_summaries, thread2_summaries, thread3_summaries) = tokio::try_join!(
            // Thread 1: IndicesStats
            async {
                let indices_stats = self.process::<IndicesStats>().await?;
                Ok::<Vec<ProcessorSummary>, eyre::Error>(vec![indices_stats])
            },
            // Thread 2: NodesStats
            async {
                let nodes_stats = self.process::<NodesStats>().await?;
                Ok::<Vec<ProcessorSummary>, eyre::Error>(vec![nodes_stats])
            },
            // Thread 3: Everything else
            async {
                let cluster_settings = self.process::<ClusterSettings>().await?;
                let health_report = self.process::<HealthReport>().await?;
                let ilm_policies = self.process::<IlmPolicies>().await?;
                let indices_settings = self.process::<IndicesSettings>().await?;
                let nodes = self.process::<Nodes>().await?;
                let pending_tasks = self.process::<PendingTasks>().await?;
                let slm_policies = self.process::<SlmPolicies>().await?;
                let tasks = self.process::<Tasks>().await?;
                Ok::<Vec<ProcessorSummary>, eyre::Error>(vec![
                    cluster_settings,
                    health_report,
                    ilm_policies,
                    indices_settings,
                    nodes,
                    pending_tasks,
                    slm_policies,
                    tasks,
                ])
            }
        )?;

        // Add all summaries to the report
        for summary in thread1_summaries
            .into_iter()
            .chain(thread2_summaries.into_iter())
            .chain(thread3_summaries.into_iter())
        {
            self.report.add_processor_summary(summary);
        }

        self.report.add_origin(
            Some(self.metadata.cluster.display_name.clone()),
            Some(self.metadata.cluster.uuid.clone()),
            Some("cluster".to_string()),
        );
        self.exporter.save_report(&self.report).await?;

        Ok(self.report)
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStream>,
    pub ilm_explain: Lookup<IlmStats>,
    pub index_settings: Lookup<IndexSettings>,
    pub node: Lookup<NodeDocument>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
