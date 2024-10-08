mod cluster_settings;
mod index_settings;
mod index_stats;
mod nodes;
mod nodes_stats;
mod searchable_snapshots_stats;
mod tasks;
use cluster_settings::ClusterSettingsProcessor;
use index_settings::IndexSettingsProcessor;
use index_stats::IndexStatsProcessor;
use nodes::NodesProcessor;
use nodes_stats::NodesStatsProcessor;
use searchable_snapshots_stats::SearchableSnapshotsStatsProcessor;
use serde_json::Value;
use tasks::TasksProcessor;

use super::{
    diagnostic::DiagnosticProcessor,
    lookup::{elasticsearch::node::NodeSummary, Lookup, LookupTable},
    DataProcessor, Metadata,
};
use crate::data::{
    diagnostic::Manifest,
    elasticsearch::{
        Alias, AliasList, Cluster, DataStream, DataStreamName, DataStreams, IlmExplain, IlmStats,
        IndexSettings, IndicesSettings, Nodes, SearchableSnapshotsCacheStats, SharedCacheStats,
    },
};
use crate::exporter::Exporter;
use crate::receiver::Receiver;
use chrono::DateTime;
use color_eyre::eyre::Result;
use futures::{
    future::{join_all, BoxFuture},
    stream::FuturesUnordered,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::RwLock, task};
use uuid::Uuid;

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Lookups,
    metadata: ElasticsearchMetadata,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    queue: Arc<RwLock<HashMap<String, Vec<Value>>>>,
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn new(manifest: Manifest, receiver: Receiver, exporter: Exporter) -> Result<Box<Self>> {
        let cluster = receiver.get::<Cluster>().await?;
        let metadata = ElasticsearchMetadata::try_new(manifest, cluster)?;

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
            lookups,
            metadata,
            queue: Arc::new(RwLock::new(HashMap::new())),
            receiver: Arc::new(receiver),
        }))
    }

    fn get_lookup(&self, key: &str) -> Option<Arc<dyn LookupTable>> {
        match key {
            "alias" => Some(Arc::new(self.lookups.alias.clone())),
            "data_stream" => Some(Arc::new(self.lookups.data_stream.clone())),
            "index_settings" => Some(Arc::new(self.lookups.index_settings.clone())),
            "node" => Some(Arc::new(self.lookups.node.clone())),
            "ilm_explain" => Some(Arc::new(self.lookups.ilm_explain.clone())),
            "shared_cache" => Some(Arc::new(self.lookups.shared_cache.clone())),
            _ => None,
        }
    }
    async fn start_exporter(&self) -> Result<()> {
        log::debug!("Starting Elasticsearch diagnostic exporter");
        Ok(())
    }

    async fn process_queue(&self) -> usize {
        let queue = self.queue.clone();
        let exporter = self.exporter.clone();

        let mut queue_guard = queue.write().await;
        let mut doc_count: usize = 0;
        for (index, docs) in queue_guard.drain() {
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

    async fn run(self) -> Result<usize> {
        log::debug!("Running Elasticsearch diagnostic processors");
        // env_logger outputs to stderr, so we can cleanly redirect stdout to a file for debugging
        if log::max_level() >= log::Level::Trace {
            println!("{}", serde_json::to_string(&self)?);
        }

        let diagnostic = Arc::new(self);

        // Run some processors and add docs to the queue
        let mut processors: Vec<BoxFuture<_>> = vec![
            Box::pin(process::<ClusterSettingsProcessor>(diagnostic.clone())),
            Box::pin(process::<IndexSettingsProcessor>(diagnostic.clone())),
            Box::pin(process::<IndexStatsProcessor>(diagnostic.clone())),
            Box::pin(process::<NodesProcessor>(diagnostic.clone())),
            Box::pin(process::<NodesStatsProcessor>(diagnostic.clone())),
            Box::pin(process::<SearchableSnapshotsStatsProcessor>(
                diagnostic.clone(),
            )),
            Box::pin(process::<TasksProcessor>(diagnostic.clone())),
        ];

        let futures = FuturesUnordered::new();
        for processor in processors.drain(..) {
            let diagnostic = diagnostic.clone();
            futures.push(task::spawn(async move {
                processor.await;
                diagnostic.process_queue().await
            }))
        }

        Ok(join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .sum())
    }
}

async fn process<T>(diag: Arc<ElasticsearchDiagnostic>)
where
    T: DataProcessor + From<Arc<ElasticsearchDiagnostic>> + 'static,
{
    let (index_name, docs) = T::from(diag.clone()).process().await;
    diag.queue.write().await.insert(index_name, docs);
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

#[derive(Clone, Serialize)]
pub struct ElasticsearchMetadata {
    pub cluster: Cluster,
    pub diagnostic: DiagnosticDoc,
    pub timestamp: i64,
    pub as_doc: MetadataDoc,
}

impl ElasticsearchMetadata {
    pub fn for_data_stream(&self, data_stream: &str) -> MetadataDoc {
        MetadataDoc {
            data_stream: DataStreamName::from(data_stream),
            ..self.as_doc.clone()
        }
    }
}

impl Metadata for ElasticsearchMetadata {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(&self.as_doc).expect("Failed to serialize metadata")
    }
}

impl ElasticsearchMetadata {
    pub fn try_new(manifest: Manifest, cluster: Cluster) -> Result<Self> {
        let collection_date = {
            if let Ok(date) = DateTime::parse_from_rfc3339(&manifest.collection_date) {
                date.timestamp_millis()
            } else if let Ok(date) =
                DateTime::parse_from_str(&manifest.collection_date, "%Y-%m-%dT%H:%M:%S%.3f%z")
            {
                date.timestamp_millis()
            } else {
                log::warn!(
                    "Failed to parse collection date: {}",
                    manifest.collection_date
                );
                chrono::Utc::now().timestamp_millis()
            }
        };

        let runner = match &manifest.runner {
            Some(runner) => runner.clone(),
            None => "Unknown".to_string(),
        };

        let diagnostic = DiagnosticDoc {
            collection_date: collection_date.clone(),
            node: cluster.name.clone(),
            runner,
            uuid: Uuid::new_v4().to_string(),
            version: manifest.diag_version.clone(),
        };

        let as_doc = MetadataDoc {
            timestamp: collection_date.clone(),
            cluster: cluster.clone(),
            diagnostic: diagnostic.clone(),
            data_stream: DataStreamName::from("metrics-default-esdiag"),
        };

        Ok(Self {
            as_doc,
            cluster,
            diagnostic,
            timestamp: collection_date.clone(),
        })
    }
}
#[derive(Clone, Serialize)]
pub struct MetadataDoc {
    #[serde(rename = "@timestamp")]
    pub timestamp: i64,
    pub cluster: Cluster,
    pub diagnostic: DiagnosticDoc,
    pub data_stream: DataStreamName,
}

impl Metadata for MetadataDoc {
    fn as_meta_doc(&self) -> Value {
        serde_json::to_value(&self).expect("Failed to serialize metadata")
    }
}

#[derive(Clone, Serialize)]
pub struct DiagnosticDoc {
    pub collection_date: i64,
    pub node: String,
    pub runner: String,
    pub uuid: String,
    pub version: Option<String>,
}
