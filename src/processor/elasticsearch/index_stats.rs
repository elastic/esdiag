use super::{
    DataProcessor, ElasticsearchMetadata, Lookups,
    lookup::{IndexSettingsDocument, NodeDocument},
    metadata::MetadataDoc,
};
use crate::data::elasticsearch::{
    Alias, Bulk, Completion, DenseVector, Docs, Fielddata, Flush, Get, IndexStats, Indexing,
    IndicesStats, Merges, QueryCache, Recovery, Refresh, RequestCache, Search, Segments,
    ShardEntry, ShardRouting, ShardStats, SparseVector, Stats, StoreSettings, StoreStats, Translog,
    Warmer,
};
use eyre::Report;
//use json_patch::merge;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::skip_serializing_none;
use std::sync::Arc;

impl DataProcessor<Lookups, ElasticsearchMetadata> for IndicesStats {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        let mut indices_stats = self.indices;
        log::debug!("index_stats indices: {}", indices_stats.len());
        let index_metadata = metadata.for_data_stream("metrics-index-esdiag");
        let shard_metadata = metadata.for_data_stream("metrics-shard-esdiag");

        let index_stats_docs: Vec<Value> = indices_stats
            .par_drain()
            .flat_map(|(index, mut index_stats)| {
                let shard_stats = index_stats.shards.take();
                let index_settings =
                    lookups
                        .index_settings
                        .by_name(&index)
                        .cloned()
                        .map(|settings| {
                            settings
                                .data_stream(lookups.data_stream.by_id(&index).cloned())
                                .age(index_metadata.diagnostic.collection_date)
                        });

                let index_settings = index_settings.map(|settings| {
                    IndexSettingsDocument::from(settings)
                        .ilm(lookups.ilm_explain.by_name(&index).cloned())
                });

                let index_stats = EnrichedIndexStats::try_from(index_stats)
                    .expect("Failed to parse index stats")
                    .alias(lookups.alias.by_name(&index).cloned())
                    .with_settings(index_settings.clone());

                let index_document =
                    IndexDocument::new(index_stats, index_metadata.clone()).calculate();
                let index_settings = index_settings
                    .map(|s| s.write_phase(index_document.index.stats.write_phase_sec));

                let shard_docs = shard_stats.map(|mut shards| {
                    shards
                        .par_drain()
                        .flat_map(|(number, mut shard_stats)| {
                            shard_stats
                                .par_drain(..)
                                .filter_map(|shard_entry| {
                                    let stats = EnrichedShardStats::try_from(shard_entry)
                                        .expect("Failed to parse shard stats")
                                        .with_id(number);
                                    let node = lookups.node.by_id(&stats.routing.node).cloned();
                                    let doc = ShardDocument::new(stats, shard_metadata.clone())
                                        .index_settings(index_settings.clone())
                                        .node(node)
                                        .calculate();
                                    serde_json::to_value(doc).ok()
                                })
                                .collect::<Vec<Value>>()
                        })
                        .collect::<Vec<Value>>()
                });

                let mut documents: Vec<Value> = vec![json!(index_document)];
                shard_docs.map(|docs| documents.extend(docs));
                documents
            })
            .collect();

        log::debug!("index_stats docs: {}", index_stats_docs.len());
        (index_metadata.data_stream.to_string(), index_stats_docs)
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
struct IndexDocument {
    index: EnrichedIndexStatsWithSettings,
    #[serde(flatten)]
    metadata: MetadataDoc,
}

impl IndexDocument {
    fn new(index: EnrichedIndexStatsWithSettings, metadata: MetadataDoc) -> Self {
        IndexDocument { index, metadata }
    }

    fn calculate(mut self) -> Self {
        let is_write_alias = self
            .index
            .stats
            .alias
            .as_ref()
            .map(|a| a.is_write_index)
            .unwrap_or(false);

        let is_write_data_stream = self
            .index
            .settings
            .as_ref()
            .and_then(|s| s.data_stream.as_ref().map(|ds| ds.is_write_index))
            .unwrap_or(false);

        self.index.stats.is_write_index = is_write_alias || is_write_data_stream;

        let stats = &mut self.index.stats;
        let collection_date = self.metadata.diagnostic.collection_date;

        if let Some(ref settings) = self.index.settings {
            let since_creation = collection_date - settings.creation_date;

            let since_rollover = settings
                .ilm
                .as_ref()
                .and_then(|ilm| ilm.lifecycle_date_millis)
                .map(|lifecycle_date| collection_date - lifecycle_date);

            let is_before_rollover = settings.ilm.as_ref().is_some_and(|ilm| {
                ilm.action
                    .as_ref()
                    .is_some_and(|action| action == "rollover")
            });

            let write_phase_sec = if let Some(rollover) = since_rollover {
                match since_creation > rollover {
                    true => (since_creation - rollover) / 1000,
                    false if is_before_rollover => since_creation / 1000,
                    _ => 0,
                }
            } else {
                0
            };

            stats.age = Some(since_creation);
            stats.since_rollover = since_rollover;
            stats.write_phase_sec = Some(write_phase_sec);
        }

        if let Some(ref mut docs) = stats.total.docs {
            let doc_avg_size = match stats.total.store.size_in_bytes {
                size if docs.count > 0 => size / docs.count,
                _ => 0,
            };

            docs.avg_size = Some(doc_avg_size);
            docs.deleted_percent =
                Some(docs.deleted as f64 / (docs.count as f64 + docs.deleted as f64));
            docs.per_gb = Some(match stats.total.store.size_in_bytes {
                0 => 0,
                1..1_073_741_824 if doc_avg_size > 0 => 1_073_741_824 / doc_avg_size,
                size => (docs.count as f32 / (size as f32 / 1_073_741_824.0)) as u64,
            });
        }

        // Estimate bytes per day

        stats.primaries.indexing.est_bytes_per_day = match stats.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(stats.primaries.store.size_in_bytes * (86_400 / seconds)),
        };

        stats.total.indexing.est_bytes_per_day = match stats.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(stats.total.store.size_in_bytes * (86_400 / seconds)),
        };

        stats.primaries.bulk.est_bytes_per_day = match stats.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(stats.primaries.bulk.total_size_in_bytes * (86_400 / seconds)),
        };

        // Determine average index time per shard

        stats.primaries.indexing.index_time_per_shard_in_millis = Some(
            stats.primaries.indexing.index_time_in_millis / stats.primaries.shard_stats.total_count,
        );

        stats.total.indexing.index_time_per_shard_in_millis =
            Some(stats.total.indexing.index_time_in_millis / stats.total.shard_stats.total_count);

        self
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
struct EnrichedIndexStats {
    pub age: Option<u64>,
    pub alias: Option<Alias>,
    pub health: Option<String>,
    pub is_write_index: bool,
    pub primaries: EnrichedStats,
    pub since_rollover: Option<u64>,
    pub total: EnrichedStats,
    pub uuid: Option<String>,
    pub write_phase_sec: Option<u64>,
}

#[skip_serializing_none]
#[derive(Serialize)]
struct EnrichedIndexStatsWithSettings {
    #[serde(flatten)]
    settings: Option<IndexSettingsDocument>,
    #[serde(flatten)]
    stats: EnrichedIndexStats,
}

impl EnrichedIndexStats {
    fn alias(self, alias: Option<Alias>) -> Self {
        Self { alias, ..self }
    }

    fn with_settings(
        self,
        settings: Option<IndexSettingsDocument>,
    ) -> EnrichedIndexStatsWithSettings {
        EnrichedIndexStatsWithSettings {
            settings,
            stats: self,
        }
    }
}

impl TryFrom<IndexStats> for EnrichedIndexStats {
    type Error = Report;

    fn try_from(stats: IndexStats) -> Result<Self, Self::Error> {
        let health = stats.health;
        let primaries = EnrichedStats::try_from(stats.primaries)?;
        let total = EnrichedStats::try_from(stats.total)?;
        let uuid = stats.uuid;

        Ok(EnrichedIndexStats {
            age: None,
            alias: None,
            health,
            is_write_index: false,
            primaries,
            since_rollover: None,
            total,
            uuid,
            write_phase_sec: None,
        })
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
struct EnrichedShardStats {
    pub commit: Value,
    pub number: u16,
    pub retention_leases: Value,
    pub routing: ShardRouting,
    pub search_idle: bool,
    pub search_idle_time: Option<u64>,
    pub seq_no: Value,
    pub shard_path: Value,
    #[serde(flatten)]
    pub stats: Stats,
}

impl EnrichedShardStats {
    pub fn with_id(self, number: u16) -> Self {
        Self { number, ..self }
    }
}

impl TryFrom<ShardEntry> for EnrichedShardStats {
    type Error = Report;

    fn try_from(entry: ShardEntry) -> Result<Self, Self::Error> {
        Ok(EnrichedShardStats {
            commit: entry.commit,
            number: 0,
            retention_leases: entry.retention_leases,
            routing: entry.routing,
            search_idle: entry.search_idle,
            search_idle_time: entry.search_idle_time,
            seq_no: entry.seq_no,
            shard_path: entry.shard_path,
            stats: entry.stats,
        })
    }
}

#[skip_serializing_none]
#[derive(Serialize)]
struct ShardDocument {
    shard: EnrichedShardStats,
    index: Option<IndexSettingsDocument>,
    node: Option<NodeDocument>,
    #[serde(flatten)]
    metadata: MetadataDoc,
}

impl ShardDocument {
    pub fn new(shard: EnrichedShardStats, metadata: MetadataDoc) -> Self {
        ShardDocument {
            shard,
            metadata,
            node: None,
            index: None,
        }
    }

    pub fn index_settings(self, index_settings: Option<IndexSettingsDocument>) -> Self {
        Self {
            index: index_settings,
            ..self
        }
    }

    pub fn node(self, node: Option<NodeDocument>) -> Self {
        Self { node, ..self }
    }

    pub fn calculate(mut self) -> Self {
        let stats = &mut self.shard.stats;
        let write_phase_sec = &self
            .index
            .as_ref()
            .map(|i| i.write_phase_sec.unwrap_or(0))
            .unwrap_or(0);

        let index_time_in_millis = &stats
            .indexing
            .as_ref()
            .map(|i| i.index_time_in_millis)
            .unwrap_or(0);
        let index_total = &stats.indexing.as_ref().map(|i| i.index_total).unwrap_or(0);
        let total_size = &stats.store.as_ref().map(|s| s.size_in_bytes).unwrap_or(0);

        // Calculated indexing stats
        if let Some(ref mut indexing) = stats.indexing {
            indexing.avg_docs_sec = Some(match write_phase_sec {
                0 => 0,
                x => index_total / x,
            });
            indexing.avg_cpu_millis = Some(match write_phase_sec {
                0 => 0,
                x => index_time_in_millis / x,
            });
            indexing.avg_bytes_sec = Some(match write_phase_sec {
                0 => 0,
                x => total_size / x,
            });
        }

        // Calculated bulk stats
        if let Some(ref mut bulk) = stats.bulk {
            bulk.compression_ratio = Some(bulk.total_size_in_bytes as f32 / *total_size as f32);
            bulk.avg_bytes_sec = Some(match write_phase_sec {
                0 => 0,
                x => bulk.total_size_in_bytes / x,
            });
            bulk.storage_ratio = Some(match bulk.total_size_in_bytes {
                0 => 0.0,
                x => *total_size as f32 / x as f32,
            });
        };

        // Calculated docs stats
        if let Some(ref mut docs) = stats.docs {
            let avg_doc_size = match total_size {
                size if docs.count > 0 => *size / docs.count,
                _ => 0,
            };
            docs.per_gb = Some(match total_size {
                0 => 0,
                1..1_073_741_824 if avg_doc_size > 0 => 1_073_741_824 / avg_doc_size,
                size => (docs.count as f32 / (*size as f32 / 1_073_741_824.0)) as u64,
            });
            docs.deleted_percent =
                Some(docs.deleted as f32 / (docs.count as f32 + docs.deleted as f32));
            docs.avg_size = Some(avg_doc_size);
        }

        // Search stats
        if let Some(ref mut search) = stats.search {
            let since_creation = self.index.as_ref().map(|i| i.age.unwrap_or(0));

            search.avg_query_cpu_millis = Some(match since_creation {
                Some(x) => search.query_time_in_millis / (x / 1000),
                None => 0,
            });
            search.avg_query_rate = Some(match since_creation {
                Some(x) => search.query_total as f64 / (x as f64 / 1000.0),
                None => 0.0,
            });
            search.avg_fetch_cpu_millis = Some(match since_creation {
                Some(x) => search.fetch_time_in_millis / (x / 1000),
                None => 0,
            });
            search.avg_fetch_rate = Some(match since_creation {
                Some(x) => search.fetch_total as f64 / (x as f64 / 1000.0),
                None => 0.0,
            });
        }

        self
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
struct EnrichedStats {
    bulk: EnrichedBulk,
    completion: Option<Completion>,
    dense_vector: Option<DenseVector>,
    docs: Option<EnrichedDocs>,
    fielddata: Option<Fielddata>,
    flush: Option<Flush>,
    get: Option<Get>,
    indexing: EnrichedIndexing,
    merges: Option<Merges>,
    query_cache: Option<QueryCache>,
    recovery: Option<Recovery>,
    refresh: Option<Refresh>,
    request_cache: Option<RequestCache>,
    search: Option<Search>,
    segments: Option<Segments>,
    shard_stats: ShardStats,
    sparse_vector: Option<SparseVector>,
    store: EnrichedStoreStats,
    translog: Option<Translog>,
    warmer: Option<Warmer>,
}

impl TryFrom<Stats> for EnrichedStats {
    type Error = Report;

    fn try_from(stats: Stats) -> Result<Self, Self::Error> {
        Ok(EnrichedStats {
            bulk: EnrichedBulk::from(stats.bulk),
            completion: stats.completion,
            dense_vector: stats.dense_vector,
            docs: stats.docs.map(|docs| EnrichedDocs::from(docs)),
            fielddata: stats.fielddata,
            flush: stats.flush,
            get: stats.get,
            indexing: EnrichedIndexing::from(stats.indexing),
            merges: stats.merges,
            query_cache: stats.query_cache,
            recovery: stats.recovery,
            refresh: stats.refresh,
            request_cache: stats.request_cache,
            search: stats.search,
            segments: stats.segments,
            shard_stats: stats.shard_stats,
            sparse_vector: stats.sparse_vector,
            store: EnrichedStoreStats::from(stats.store),
            translog: stats.translog,
            warmer: stats.warmer,
        })
    }
}

#[skip_serializing_none]
#[derive(Default, Deserialize, Serialize)]
struct EnrichedStoreStats {
    size_in_bytes: u64,
    total_data_set_size_in_bytes: Option<u64>,
    reserved_in_bytes: Option<u64>,
    #[serde(flatten)]
    settings: Option<StoreSettings>,
}

impl From<Option<StoreStats>> for EnrichedStoreStats {
    fn from(stats: Option<StoreStats>) -> Self {
        match stats {
            Some(stats) => EnrichedStoreStats {
                size_in_bytes: stats.size_in_bytes,
                total_data_set_size_in_bytes: stats.total_data_set_size_in_bytes,
                reserved_in_bytes: stats.reserved_in_bytes,
                settings: None,
            },
            None => EnrichedStoreStats::default(),
        }
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct EnrichedDocs {
    pub count: u64,
    pub deleted: u64,
    pub total_size_in_bytes: u64,
    // Calculated fields
    pub per_gb: Option<u64>,
    pub avg_size: Option<u64>,
    pub deleted_percent: Option<f64>,
}

impl From<Docs> for EnrichedDocs {
    fn from(docs: Docs) -> Self {
        EnrichedDocs {
            count: docs.count,
            deleted: docs.deleted,
            total_size_in_bytes: docs.total_size_in_bytes,
            per_gb: None,
            avg_size: None,
            deleted_percent: None,
        }
    }
}

#[skip_serializing_none]
#[derive(Default, Deserialize, Serialize)]
struct EnrichedIndexing {
    index_total: u64,
    index_time_in_millis: u64,
    index_current: u64,
    index_failed: u64,
    delete_current: u64,
    delete_time_in_millis: u64,
    delete_total: u64,
    is_throttled: bool,
    noop_update_total: u64,
    throttle_time_in_millis: u64,
    write_load: f64,
    // Calculated fields
    est_bytes_per_day: Option<u64>,
    index_time_per_shard_in_millis: Option<u64>,
}

impl From<Option<Indexing>> for EnrichedIndexing {
    fn from(indexing: Option<Indexing>) -> Self {
        match indexing {
            Some(indexing) => EnrichedIndexing {
                delete_current: indexing.delete_current,
                delete_time_in_millis: indexing.delete_time_in_millis,
                delete_total: indexing.delete_total,
                index_current: indexing.index_current,
                index_failed: indexing.index_failed,
                index_time_in_millis: indexing.index_time_in_millis,
                index_total: indexing.index_total,
                is_throttled: indexing.is_throttled.unwrap_or(false),
                noop_update_total: indexing.noop_update_total.unwrap_or(0),
                throttle_time_in_millis: indexing.throttle_time_in_millis.unwrap_or(0),
                write_load: indexing.write_load.unwrap_or(0.0),
                // Calculated Fields
                est_bytes_per_day: None,
                index_time_per_shard_in_millis: None,
            },
            None => EnrichedIndexing::default(),
        }
    }
}

#[skip_serializing_none]
#[derive(Default, Deserialize, Serialize)]
pub struct EnrichedBulk {
    avg_size_in_bytes: u64,
    avg_time_in_millis: u64,
    total_operations: u64,
    total_size_in_bytes: u64,
    total_time_in_millis: u64,
    // Calculated Fields
    est_bytes_per_day: Option<u64>,
}

impl From<Option<Bulk>> for EnrichedBulk {
    fn from(bulk: Option<Bulk>) -> Self {
        match bulk {
            Some(bulk) => EnrichedBulk {
                avg_size_in_bytes: bulk.avg_size_in_bytes.unwrap_or(0),
                avg_time_in_millis: bulk.avg_time_in_millis.unwrap_or(0),
                total_operations: bulk.total_operations,
                total_size_in_bytes: bulk.total_size_in_bytes,
                total_time_in_millis: bulk.total_time_in_millis,
                est_bytes_per_day: None,
            },
            None => EnrichedBulk::default(),
        }
    }
}
