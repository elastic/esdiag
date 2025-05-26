use super::{DataProcessor, ElasticsearchMetadata, Lookups, metadata::MetadataDoc};
use crate::data::elasticsearch::{
    Alias, Bulk, Completion, DataStream, DenseVector, Docs, Fielddata, Flush, Get, IlmStats,
    IndexSettings, IndexStats, Indexing, IndicesStats, Merges, QueryCache, Recovery, Refresh,
    RequestCache, Search, Segments, ShardStats, SparseVector, Stats, StoreSettings, StoreStats,
    Translog, Warmer,
};
use eyre::Report;
//use json_patch::merge;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

impl DataProcessor<Lookups, ElasticsearchMetadata> for IndicesStats {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        let mut indices_stats = self.indices;
        log::debug!("index_stats indices: {}", indices_stats.len());
        let data_stream = "metrics-index-esdiag".to_string();
        let index_metadata = metadata.for_data_stream(&data_stream);
        // let shard_metadata = metadata.for_data_stream("metrics-shard-esdiag");
        let lookup = lookups;

        let indices_stats: Vec<Value> = indices_stats
            .par_drain()
            .flat_map(|(index, index_stats)| {
                // let mut shard_stats: Vec<(String, Value)> = match index_stats.shards.take() {
                //     Some(shards) => shards.par_drain().collect(),
                //     None => Vec::new(),
                // };

                let index_stats = EnrichedIndexStats::try_from(index_stats)
                    .expect("Failed to parse index stats")
                    .name(index.clone())
                    .data_stream(lookup.data_stream.by_id(&index).cloned())
                    .settings(lookup.index_settings.by_name(&index).cloned())
                    .alias(lookup.alias.by_name(&index).cloned())
                    .ilm(lookup.ilm_explain.by_name(&index).cloned());

                let index_document =
                    IndexDocument::new(index_stats, index_metadata.clone()).calculate();

                let mut shard_docs = Vec::new();
                // let mut shard_docs: Vec<_> = shard_stats
                //     .par_drain(..)
                //     .flat_map(|(shard_id, shard_stats)| {
                //         let mut shard_doc = index_doc.clone();

                //         merge(&mut shard_doc, &shard_metadata);
                //         merge(&mut shard_doc, &json!({"shard": { "number": shard_id, }}));
                //         let shard_docs: Vec<Value> = shard_stats
                //             .as_array()
                //             .expect("Failed to get shard_stats array")
                //             .par_iter()
                //             .map(|shard_stats| {
                //                 let write_phase_sec = index_stats.write_phase_sec.unwrap_or(0);
                //                 let node = lookup
                //                     .node
                //                     .by_id(shard_stats["routing"]["node"].as_str().unwrap_or(""));

                //                 // Indexing stats
                //                 let index_time_in_millis =
                //                     &shard_stats["indexing"]["index_time_in_millis"]
                //                         .as_u64()
                //                         .unwrap_or(0);
                //                 let index_total =
                //                     &shard_stats["indexing"]["index_total"].as_u64().unwrap_or(0);
                //                 let total_size =
                //                     &shard_stats["store"]["size_in_bytes"].as_u64().unwrap_or(0);
                //                 let bulk_size = &shard_stats["bulk"]["total_size_in_bytes"]
                //                     .as_u64()
                //                     .unwrap_or(0);

                //                 let avg_docs_sec = match write_phase_sec {
                //                     0 => 0,
                //                     x => index_total / x,
                //                 };
                //                 let index_avg_cpu_millis = match write_phase_sec {
                //                     0 => 0,
                //                     x => index_time_in_millis / x,
                //                 };
                //                 let indexing_avg_bytes_sec = match write_phase_sec {
                //                     0 => 0,
                //                     x => total_size / x,
                //                 };
                //                 let bulk_avg_bytes_sec = match write_phase_sec {
                //                     0 => 0,
                //                     x => bulk_size / x,
                //                 };
                //                 let storage_ratio = match bulk_size {
                //                     0 => 0.0,
                //                     x => *total_size as f32 / *x as f32,
                //                 };
                //                 let compression_ratio = *bulk_size as f32 / *total_size as f32;

                //                 // Search stats
                //                 let query_time_in_millis =
                //                     &shard_stats["search"]["query_time_in_millis"]
                //                         .as_u64()
                //                         .unwrap_or(0);
                //                 let query_total =
                //                     &shard_stats["search"]["query_total"].as_f64().unwrap_or(0.0);
                //                 let fetch_time_in_millis =
                //                     &shard_stats["search"]["fetch_time_in_millis"]
                //                         .as_u64()
                //                         .unwrap_or(0);
                //                 let fetch_total =
                //                     &shard_stats["search"]["fetch_total"].as_f64().unwrap_or(0.0);
                //                 let avg_query_cpu_millis = match since_creation {
                //                     Some(x) => query_time_in_millis / (x / 1000),
                //                     None => 0,
                //                 };
                //                 let avg_query_rate = match since_creation {
                //                     Some(x) => query_total / (x as f64 / 1000.0),
                //                     None => 0.0,
                //                 };
                //                 let avg_fetch_cpu_millis = match since_creation {
                //                     Some(x) => fetch_time_in_millis / (x / 1000),
                //                     None => 0,
                //                 };
                //                 let avg_fetch_rate = match since_creation {
                //                     Some(x) => fetch_total / (x as f64 / 1000.0),
                //                     None => 0.0,
                //                 };
                //                 let doc_count = &shard_stats["docs"]["count"].as_u64().unwrap_or(0);
                //                 let avg_doc_size = match total_size {
                //                     size if *doc_count > 0 => *size / *doc_count,
                //                     _ => 0,
                //                 };
                //                 let docs_per_gb = match total_size {
                //                     0 => 0,
                //                     1..1_073_741_824 if avg_doc_size > 0 => {
                //                         1_073_741_824 / avg_doc_size
                //                     }
                //                     size => {
                //                         (*doc_count as f32 / (*size as f32 / 1_073_741_824.0))
                //                             as u64
                //                     }
                //                 };
                //                 let docs_deleted_percent = {
                //                     let deleted_count =
                //                         shard_stats["docs"]["deleted"].as_f64().unwrap_or(0.0);
                //                     deleted_count / (*doc_count as f64 + deleted_count)
                //                 };

                //                 // Patch new calculated stats
                //                 let indexing_patch = json!({
                //                     "shard": {
                //                         "docs": {
                //                             "per_gb": docs_per_gb,
                //                             "avg_size": avg_doc_size,
                //                             "deleted_percent": docs_deleted_percent,
                //                         },
                //                         "indexing": {
                //                             "avg_docs_sec": avg_docs_sec,
                //                             "avg_cpu_millis": index_avg_cpu_millis,
                //                             "avg_bytes_sec": indexing_avg_bytes_sec,
                //                             "storage_ratio": storage_ratio,
                //                         },
                //                         "bulk": {
                //                             "compression_ratio": compression_ratio,
                //                             "avg_bytes_sec": bulk_avg_bytes_sec,
                //                         },
                //                         "search": {
                //                             "avg_query_cpu_millis": avg_query_cpu_millis,
                //                             "avg_query_rate": avg_query_rate,
                //                             "avg_fetch_cpu_millis": avg_fetch_cpu_millis,
                //                             "avg_fetch_rate": avg_fetch_rate,
                //                         }
                //                     }
                //                 });

                //                 let mut doc = json!({
                //                     "shard": shard_stats,
                //                     "node": node,
                //                 });
                //                 merge(&mut doc, &indexing_patch);
                //                 merge(&mut doc, &shard_doc);
                //                 doc
                //             })
                //             .collect();
                //         shard_docs
                //     })
                //     .collect();

                //merge(&mut doc, &index_doc);
                //merge(&mut doc, &index_metadata);
                //merge(&mut doc, &stats_patch);
                shard_docs.insert(0, json!(index_document));
                shard_docs
            })
            .collect();

        log::debug!("index_stats docs: {}", indices_stats.len());
        (data_stream, indices_stats)
    }
}

#[derive(Serialize)]
struct IndexDocument {
    index: EnrichedIndexStats,
    #[serde(flatten)]
    metadata: MetadataDoc,
}

impl IndexDocument {
    fn new(index: EnrichedIndexStats, metadata: MetadataDoc) -> Self {
        IndexDocument { index, metadata }
    }

    fn calculate(mut self) -> Self {
        let collection_date = self.metadata.diagnostic.collection_date;
        let since_creation = self
            .index
            .creation_date
            .map(|creation_date| collection_date - creation_date);
        let since_rollover = self
            .index
            .ilm
            .as_ref()
            .and_then(|ilm| ilm.lifecycle_date_millis)
            .map(|lifecycle_date| collection_date - lifecycle_date);
        let is_before_rollover = self.index.ilm.as_ref().is_some_and(|ilm| {
            ilm.action
                .as_ref()
                .is_some_and(|action| action == "rollover")
        });

        let write_phase_sec =
            if let (Some(creation), Some(rollover)) = (since_creation, since_rollover) {
                match creation > rollover {
                    true => (creation - rollover) / 1000,
                    false if is_before_rollover => creation / 1000,
                    _ => 0,
                }
            } else {
                0
            };

        if let Some(ref mut docs) = self.index.total.docs {
            let doc_avg_size = match self.index.total.store.size_in_bytes {
                size if docs.count > 0 => size / docs.count,
                _ => 0,
            };

            docs.avg_size = Some(doc_avg_size);
            docs.deleted_percent =
                Some(docs.deleted as f64 / (docs.count as f64 + docs.deleted as f64));
            docs.per_gb = Some(match self.index.total.store.size_in_bytes {
                0 => 0,
                1..1_073_741_824 if doc_avg_size > 0 => 1_073_741_824 / doc_avg_size,
                size => (docs.count as f32 / (size as f32 / 1_073_741_824.0)) as u64,
            });
        }

        // Estimate bytes per day

        self.index.primaries.indexing.est_bytes_per_day = match self.index.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(self.index.primaries.store.size_in_bytes * (86_400 / seconds)),
        };

        self.index.total.indexing.est_bytes_per_day = match self.index.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(self.index.total.store.size_in_bytes * (86_400 / seconds)),
        };

        self.index.primaries.bulk.est_bytes_per_day = match self.index.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => {
                Some(self.index.primaries.bulk.total_size_in_bytes * (86_400 / seconds))
            }
        };

        // Determine average index time per shard

        self.index.primaries.indexing.index_time_per_shard_in_millis = Some(
            self.index.primaries.indexing.index_time_in_millis
                / self.index.primaries.shard_stats.total_count,
        );

        self.index.total.indexing.index_time_per_shard_in_millis = Some(
            self.index.total.indexing.index_time_in_millis
                / self.index.total.shard_stats.total_count,
        );

        let collection_date = self.metadata.diagnostic.collection_date;

        self.index.age = self
            .index
            .creation_date
            .map(|creation_date| collection_date - creation_date);
        self.index.since_creation = since_creation;
        self.index.since_rollover = since_rollover;
        self.index.write_phase_sec = Some(write_phase_sec);
        self
    }
}

#[derive(Serialize)]
struct EnrichedIndexStats {
    age: Option<u64>,
    alias: Option<Alias>,
    codec: Option<String>,
    creation_date: Option<u64>,
    data_stream: Option<DataStream>,
    health: Option<String>,
    ilm: Option<IlmStats>,
    is_write_index: bool,
    lifecycle: Option<Value>,
    mode: Option<String>,
    name: String,
    number_of_replicas: Option<u64>,
    number_of_shards: Option<u64>,
    refresh_interval: Option<String>,
    primaries: EnrichedStats,
    since_creation: Option<u64>,
    since_rollover: Option<u64>,
    store: Option<StoreSettings>,
    source: Option<String>,
    total: EnrichedStats,
    uuid: Option<String>,
    write_phase_sec: Option<u64>,
}

impl EnrichedIndexStats {
    fn alias(self, alias: Option<Alias>) -> Self {
        let is_alias_write_index = alias.as_ref().map_or(false, |a| a.is_write_index);
        Self {
            alias,
            is_write_index: self.is_write_index || is_alias_write_index,
            ..self
        }
    }

    fn data_stream(self, data_stream: Option<DataStream>) -> Self {
        let is_data_stream_write_index = data_stream.as_ref().map_or(false, |ds| ds.is_write_index);
        Self {
            data_stream,
            is_write_index: self.is_write_index || is_data_stream_write_index,
            ..self
        }
    }

    fn ilm(self, ilm: Option<IlmStats>) -> Self {
        Self { ilm, ..self }
    }

    fn name(self, name: String) -> Self {
        Self { name, ..self }
    }

    fn settings(self, settings: Option<IndexSettings>) -> Self {
        if let Some(settings) = settings {
            Self {
                codec: Some(settings.codec),
                creation_date: settings.creation_date,
                lifecycle: settings.lifecycle,
                mode: Some(settings.mode),
                store: settings.store,
                number_of_shards: settings.number_of_shards,
                number_of_replicas: settings.number_of_replicas,
                refresh_interval: Some(settings.refresh_interval),
                source: settings.source,
                ..self
            }
        } else {
            self
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
            codec: None,
            creation_date: None,
            data_stream: None,
            health,
            ilm: None,
            is_write_index: false,
            lifecycle: None,
            mode: None,
            name: "".to_string(),
            number_of_replicas: None,
            number_of_shards: None,
            primaries,
            refresh_interval: None,
            since_creation: None,
            since_rollover: None,
            store: None,
            source: None,
            total,
            uuid,
            write_phase_sec: None,
        })
    }
}

#[derive(Deserialize, Serialize)]
struct EnrichedShardStats {
    shards: HashMap<String, Value>,
}

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
            count: docs.count.unwrap_or(0),
            deleted: docs.deleted.unwrap_or(0),
            total_size_in_bytes: docs.total_size_in_bytes.unwrap_or(0),
            per_gb: None,
            avg_size: None,
            deleted_percent: None,
        }
    }
}

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
                delete_current: indexing.delete_current.unwrap_or(0),
                delete_time_in_millis: indexing.delete_time_in_millis.unwrap_or(0),
                delete_total: indexing.delete_total.unwrap_or(0),
                index_current: indexing.index_current.unwrap_or(0),
                index_failed: indexing.index_failed.unwrap_or(0),
                index_time_in_millis: indexing.index_time_in_millis.unwrap_or(0),
                index_total: indexing.index_total.unwrap_or(0),
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
                total_operations: bulk.total_operations.unwrap_or(0),
                total_size_in_bytes: bulk.total_size_in_bytes.unwrap_or(0),
                total_time_in_millis: bulk.total_time_in_millis.unwrap_or(0),
                est_bytes_per_day: None,
            },
            None => EnrichedBulk::default(),
        }
    }
}
