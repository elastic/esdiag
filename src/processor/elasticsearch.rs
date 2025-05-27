/// Processor for the `_cluster/settings` API
mod cluster_settings;
/// Processor for the `_settings` API
mod index_settings;
/// Processor for the `_stats` API
mod index_stats;
/// Processor for Elasticsearch lookups
mod lookup;
/// Processor for Elasticsearch diagnostics metadata
mod metadata;
/// Processor for the `_nodes` API
mod nodes;
/// Processor for the `_nodes/stats` API
mod nodes_stats;
/// Processor for the `_searchable_snapshots/stats` API
mod searchable_snapshots_stats;
/// Processor for the `_tasks` API
mod tasks;

use super::{DataProcessor, DiagnosticProcessor, Metadata};
use crate::{
    data::{
        self,
        diagnostic::{
            DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder, Lookup,
            Product, elasticsearch::DataSet, report::ProcessorSummary,
        },
        elasticsearch::{
            Alias, AliasList, Cluster, ClusterSettings, DataStream, DataStreams, IlmExplain,
            IlmStats, IndexSettings, IndicesSettings, IndicesStats, Nodes, NodesStats,
            SearchableSnapshotsCacheStats, SharedCacheStats, Tasks,
        },
    },
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use futures::{future::join_all, stream::FuturesUnordered};
use lookup::NodeDocument;
use metadata::ElasticsearchMetadata;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::{sync::RwLock, task::JoinHandle};

type ExporterDocumentQueue = Arc<RwLock<Vec<(String, Vec<Value>)>>>;

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Arc<Lookups>,
    metadata: Arc<ElasticsearchMetadata>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    report: Arc<RwLock<DiagnosticReport>>,
    #[serde(skip)]
    queue: ExporterDocumentQueue,
}

impl ElasticsearchDiagnostic {
    async fn process_queue(&self, name: String) -> Option<ProcessorSummary> {
        let queue = self.queue.clone();
        let exporter = self.exporter.clone();

        let mut queue_guard = queue.write().await;
        if let Some((index, docs)) = queue_guard.pop() {
            log::debug!("Processing queue {index}");
            exporter
                .write(index, docs)
                .await
                .ok()
                .map(|summary| summary.rename(name).was_parsed())
        } else {
            log::warn!("Queue was empty");
            None
        }
    }
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
        let cluster = receiver.get::<Cluster>().await?;
        let display_name = receiver.get::<ClusterSettings>().await?.get_display_name();
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

        report.add_lookup("alias", &lookups.alias);
        report.add_lookup("data_stream", &lookups.data_stream);
        report.add_lookup("index_settings", &lookups.index_settings);
        report.add_lookup("node", &lookups.node);
        report.add_lookup("ilm_explain", &lookups.ilm_explain);
        report.add_lookup("shared_cache", &lookups.shared_cache);

        Ok(Box::new(Self {
            exporter: Arc::new(exporter),
            lookups: Arc::new(lookups),
            metadata: Arc::new(metadata.clone()),
            queue: Arc::new(RwLock::new(Vec::<(String, Vec<Value>)>::new())),
            receiver: Arc::new(receiver),
            report: Arc::new(RwLock::new(report)),
        }))
    }

    async fn run(self) -> Result<()> {
        log::debug!("Running Elasticsearch diagnostic processors");
        if let false = self.exporter.is_connected().await {
            return Err(eyre!("Exporter is not connected"));
        }

        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let diag = Arc::new(self);

        let futures = FuturesUnordered::new();
        let mut tasks = HashMap::from([
            (
                DataSet::ClusterSettings,
                spawn_processor::<ClusterSettings>(diag.clone()),
            ),
            (
                DataSet::IndicesSettings,
                spawn_processor::<IndicesSettings>(diag.clone()),
            ),
            (
                DataSet::IndicesStats,
                spawn_processor::<IndicesStats>(diag.clone()),
            ),
            (DataSet::Nodes, spawn_processor::<Nodes>(diag.clone())),
            (
                DataSet::NodesStats,
                spawn_processor::<NodesStats>(diag.clone()),
            ),
            // Temporarily omitting in favor of an include/exclude/diag_type filter to
            // prevent the expected error
            // (
            // DataSet::SearchableSnapshotsStats,
            // spawn_processor::<SearchableSnapshotsStats>(diag.clone()),
            // ),
            (DataSet::Tasks, spawn_processor::<Tasks>(diag.clone())),
        ]);
        tasks
            .drain()
            .map(|(_name, task)| futures.push(task))
            .count();

        let mut report = diag.report.write().await;
        join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .flatten()
            .for_each(|summary| report.add_processor_summary(summary));

        log::info!(
            "Created {} documents for diagnostic: {}",
            report.docs.created,
            report.metadata.id,
        );
        diag.exporter.save_report(&*report).await?;

        Ok(())
    }
}

type DataProcessorTask = Pin<Box<JoinHandle<Option<ProcessorSummary>>>>;

fn spawn_processor<T>(diagnostic: Arc<ElasticsearchDiagnostic>) -> DataProcessorTask
where
    T: DataSource + DataProcessor<Lookups, ElasticsearchMetadata> + DeserializeOwned + Send + Sync,
{
    let lookups = diagnostic.lookups.clone();
    let metadata = diagnostic.metadata.clone();
    Box::pin(tokio::task::spawn(async move {
        let docs = diagnostic
            .receiver
            .get::<T>()
            .await
            .map(|data| data.generate_docs(lookups, metadata));
        match docs {
            Ok(docs) => {
                diagnostic.queue.write().await.push(docs);
                diagnostic.process_queue(T::name()).await
            }
            Err(e) => {
                log::warn!("No {} data found: {e}", T::name());
                Some(ProcessorSummary::new(T::name()))
            }
        }
    }))
}

#[derive(Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStream>,
    pub index_settings: Lookup<IndexSettings>,
    pub node: Lookup<NodeDocument>,
    pub ilm_explain: Lookup<IlmStats>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
