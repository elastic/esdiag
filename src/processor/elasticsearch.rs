/// Processor for the `_cluster/settings` API
mod cluster_settings;
/// Processor for the `_settings` API
mod index_settings;
/// Processor for the `_stats` API
mod index_stats;
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

use super::{
    lookup::{elasticsearch::node::NodeSummary, Lookup},
    DataProcessor, DiagnosticProcessor, Metadata,
};
use crate::{
    data::{
        self,
        diagnostic::{data_source::DataSource, DiagnosticManifest},
        elasticsearch::{
            Alias, AliasList, Cluster, ClusterSettings, DataStream, DataStreams, IlmExplain,
            IlmStats, IndexSettings, IndicesSettings, IndicesStats, Nodes, NodesStats,
            SearchableSnapshotsCacheStats, SearchableSnapshotsStats, SharedCacheStats, Tasks,
        },
    },
    exporter::Exporter,
    receiver::Receiver,
};
use color_eyre::eyre::Result;
use futures::{future::join_all, stream::FuturesUnordered};
use metadata::ElasticsearchMetadata;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{pin::Pin, sync::Arc};
use tokio::{sync::RwLock, task::JoinHandle};

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Arc<Lookups>,
    metadata: Arc<ElasticsearchMetadata>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    queue: Arc<RwLock<Vec<(String, Vec<Value>)>>>,
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

        let lookups = Lookups {
            alias: Lookup::from(receiver.get::<AliasList>().await?),
            data_stream: Lookup::from(receiver.get::<DataStreams>().await?),
            index_settings: Lookup::from(receiver.get::<IndicesSettings>().await?),
            node: Lookup::from(receiver.get::<Nodes>().await?),
            ilm_explain: Lookup::from(receiver.get::<IlmExplain>().await?),
            shared_cache: Lookup::from(receiver.get::<SearchableSnapshotsCacheStats>().await?),
        };

        Ok(Box::new(Self {
            exporter: Arc::new(exporter),
            lookups: Arc::new(lookups),
            metadata: Arc::new(metadata),
            queue: Arc::new(RwLock::new(Vec::<(String, Vec<Value>)>::new())),
            receiver: Arc::new(receiver),
        }))
    }

    async fn process_queue(&self) -> usize {
        let queue = self.queue.clone();
        let exporter = self.exporter.clone();

        let mut queue_guard = queue.write().await;
        let mut doc_count: usize = 0;
        for (index, docs) in queue_guard.drain(..) {
            log::debug!("Processing queue {index}");
            if docs.is_empty() {
                continue;
            }
            match exporter.write(index, docs).await {
                Ok(count) => doc_count += count,
                Err(e) => log::error!("Elasticsearch exporter: {e}"),
            }
        }
        doc_count
    }

    async fn run(self) -> Result<(String, usize)> {
        log::debug!("Running Elasticsearch diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let diag = Arc::new(self);

        let futures = FuturesUnordered::new();
        let mut tasks = vec![
            spawn_processor::<ClusterSettings>(diag.clone()),
            spawn_processor::<IndicesSettings>(diag.clone()),
            spawn_processor::<IndicesStats>(diag.clone()),
            spawn_processor::<Nodes>(diag.clone()),
            spawn_processor::<NodesStats>(diag.clone()),
            spawn_processor::<SearchableSnapshotsStats>(diag.clone()),
            spawn_processor::<Tasks>(diag.clone()),
        ];
        tasks.drain(..).map(|task| futures.push(task)).count();

        let doc_count = join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .sum();
        let diag_id = diag.metadata.diagnostic.id.clone();

        Ok((diag_id, doc_count))
    }
}

type DataProcessorTask = Pin<Box<JoinHandle<usize>>>;

fn spawn_processor<T>(diagnostic: Arc<ElasticsearchDiagnostic>) -> DataProcessorTask
where
    T: DataSource + DataProcessor<ElasticsearchMetadata> + DeserializeOwned + Send + Sync,
{
    let lookups = diagnostic.lookups.clone();
    let metadata = diagnostic.metadata.clone();
    Box::pin(tokio::task::spawn(async move {
        let docs = diagnostic
            .receiver
            .get::<T>()
            .await
            .ok()
            .map(|data| data.generate_docs(lookups, metadata));
        match docs {
            Some(docs) => {
                diagnostic.queue.write().await.push(docs);
                diagnostic.process_queue().await
            }
            None => {
                log::warn!("No {} data found", T::name());
                0
            }
        }
    }))
}

#[derive(Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStream>,
    pub index_settings: Lookup<IndexSettings>,
    pub node: Lookup<NodeSummary>,
    pub ilm_explain: Lookup<IlmStats>,
    pub shared_cache: Lookup<SharedCacheStats>,
}
