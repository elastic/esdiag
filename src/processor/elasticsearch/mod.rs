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
use tokio::{sync::mpsc, time::Instant};
pub use {
    licenses::License,
    version::{Cluster, Version},
};

use super::{
    DataSource, DiagnosticManifest, DiagnosticProcessor, DiagnosticReport, DocumentExporter,
    Metadata, ProcessorSummary, Product,
    diagnostic::{DiagnosticReportBuilder, Lookup},
    elasticsearch::health_report::HealthReport,
};
use crate::{data, exporter::Exporter, processor::BatchResponse, receiver::Receiver};
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
    async fn process_datasource<T>(
        &self,
        batch_tx: mpsc::Sender<BatchResponse>,
    ) -> Result<ProcessorSummary>
    where
        T: DataSource
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
    {
        let summary = match self.receiver.get::<T>().await {
            Ok(data) => data
                .documents_export(&self.exporter, &self.lookups, &self.metadata, batch_tx)
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

    async fn process(
        self,
        start_time: &Instant,
        batch_tx: mpsc::Sender<BatchResponse>,
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()> {
        log::debug!("Running Elasticsearch diagnostic processors");
        if self.exporter.is_connected().await == false {
            return Err(eyre!("Exporter is not connected"));
        }

        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        // self.report.add_identifiers(self.exporter.identifiers());

        let (thread1_summaries, thread2_summaries, thread3_summaries) = tokio::try_join!(
            // Thread 1: IndicesStats
            async {
                let indices_stats = self
                    .process_datasource::<IndicesStats>(batch_tx.clone())
                    .await?;
                Ok::<Vec<ProcessorSummary>, eyre::Error>(vec![indices_stats])
            },
            // Thread 2: NodesStats
            async {
                let nodes_stats = self
                    .process_datasource::<NodesStats>(batch_tx.clone())
                    .await?;
                Ok::<Vec<ProcessorSummary>, eyre::Error>(vec![nodes_stats])
            },
            // Thread 3: Everything else
            async {
                let cluster_settings = self
                    .process_datasource::<ClusterSettings>(batch_tx.clone())
                    .await?;
                let health_report = self
                    .process_datasource::<HealthReport>(batch_tx.clone())
                    .await?;
                let ilm_policies = self
                    .process_datasource::<IlmPolicies>(batch_tx.clone())
                    .await?;
                let indices_settings = self
                    .process_datasource::<IndicesSettings>(batch_tx.clone())
                    .await?;
                let nodes = self.process_datasource::<Nodes>(batch_tx.clone()).await?;
                let pending_tasks = self
                    .process_datasource::<PendingTasks>(batch_tx.clone())
                    .await?;
                let slm_policies = self
                    .process_datasource::<SlmPolicies>(batch_tx.clone())
                    .await?;
                let tasks = self.process_datasource::<Tasks>(batch_tx.clone()).await?;
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
            summary_tx.send(summary).await?;
        }

        // self.report.add_origin(
        // Some(self.metadata.cluster.display_name.clone()),
        // Some(self.metadata.cluster.uuid.clone()),
        // Some("cluster".to_string()),
        // );
        // self.exporter.save_report(&self.report).await?;

        // log::info!(
        // "Created {} documents for {} diagnostic: {}",
        // self.report.docs.created,
        // self.report.product,
        // self.report.metadata.id,
        // );
        // if let Ok(kibana_url) = std::env::var("ESDIAG_KIBANA_URL") {
        // let kibana_link = format!(
        // "{}/app/dashboards#/view/4e0a26b2-e5f8-4b58-b617-86f5cdd0edad?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'4319ebc4-df81-4b18-b8bd-6aaa55a1fd13',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:(from:now-90d,to:now))",
        // kibana_url, self.report.metadata.id, self.report.metadata.id
        // );
        // log::info!("{}", kibana_link);
        // self.report.add_kibana_link(kibana_link);
        // }
        // self.report
        // .add_processing_duration(start_time.elapsed().as_millis());
        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStreamDocument>,
    pub ilm_explain: Lookup<IlmStats>,
    pub index_settings: Lookup<IndexSettings>,
    pub node: Lookup<NodeDocument>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
