// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{Exporter, ProcessorSummary};
use super::super::{
    DocumentExporter, ElasticsearchMetadata, Lookup, Lookups,
    alias::Alias,
    indices_settings::{IndexSettingsDocument, StoreSettings},
    metadata::MetadataDoc,
    nodes::NodeDocument,
};
use super::{IndicesStats, data::*};
use eyre::Report;

//use json_patch::merge;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use tokio::sync::mpsc;

impl DocumentExporter<Lookups, ElasticsearchMetadata> for IndicesStats {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("index_stats indices: {}", self.indices.len());
        let indices_stats = self.indices;
        let data_stream_name = "metrics-index-esdiag".to_string();
        let index_metadata = metadata.for_data_stream(&data_stream_name);
        let shard_metadata = metadata.for_data_stream("metrics-shard-esdiag");

        // Tune batch sizes and channel buffers for memory usage and write frequency
        let batch_size = 5000;
        const BUFFER_SIZE: usize = 5000;

        // Spawn document channels for concurrent processing with backpressure
        let (index_tx, index_rx) = mpsc::channel::<IndexStatsDocument>(BUFFER_SIZE);
        let index_processor =
            tokio::spawn(exporter.clone().document_channel::<IndexStatsDocument>(
                index_rx,
                index_metadata.data_stream.to_string(),
                batch_size,
            ));

        let (shard_tx, shard_rx) = mpsc::channel::<ShardStatsDocument>(BUFFER_SIZE);
        let shard_processor =
            tokio::spawn(exporter.clone().document_channel::<ShardStatsDocument>(
                shard_rx,
                shard_metadata.data_stream.to_string(),
                batch_size,
            ));

        for (index_name, mut index_stats) in indices_stats {
            let shards_stats = index_stats.shards.take();
            let index_settings =
                lookups
                    .index_settings
                    .by_name(&index_name)
                    .cloned()
                    .map(|settings| {
                        settings
                            .data_stream(lookups.data_stream.by_id(&index_name).cloned())
                            .age(metadata.diagnostic.collection_date)
                    });

            let index_settings = index_settings.map(|settings| {
                IndexSettingsDocument::from(settings)
                    .ilm(lookups.ilm_explain.by_name(&index_name).cloned())
            });

            let write_phase_sec = match EnrichedIndexStats::try_from(index_stats) {
                Ok(enriched_stats) => {
                    let stats = enriched_stats
                        .name(index_name.clone())
                        .alias(lookups.alias.by_name(&index_name).cloned())
                        .with_settings(index_settings.clone());
                    let index_document =
                        IndexStatsDocument::new(stats, index_metadata.clone()).calculate();
                    let write_phase_sec = index_document.index.stats.write_phase_sec;
                    if let Err(_) = index_tx.send(index_document).await {
                        log::warn!("Index channel closed unexpectedly");
                    }
                    write_phase_sec
                }
                Err(_) => {
                    log::warn!("Failed to create index document");
                    None
                }
            };

            let index_settings = index_settings.map(|s| s.write_phase(write_phase_sec));

            if let Some(shards) = shards_stats {
                match extract_shard_documents(
                    shards,
                    &shard_metadata,
                    index_name,
                    index_settings,
                    &lookups.node,
                ) {
                    Ok(docs) => {
                        for doc in docs {
                            if shard_tx.send(doc).await.is_err() {
                                log::warn!("Shard channel closed unexpectedly");
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!("Failed to create shard documents: {}", err);
                    }
                }
            }
        }

        // Close channels to signal completion
        drop(index_tx);
        drop(shard_tx);

        // Wait for processors to complete
        let (index_result, shard_result) = tokio::join!(index_processor, shard_processor);

        // Merge summaries

        let mut summary = ProcessorSummary::new(data_stream_name);
        summary.merge(index_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(shard_result.map_err(|err| eyre::Report::new(err)));

        log::debug!("indices_stats processed: {}", summary.docs);
        summary
    }
}

fn extract_shard_documents(
    mut shards: std::collections::HashMap<u16, Vec<ShardEntry>>,
    shard_metadata: &MetadataDoc,
    index_name: String,
    index_settings: Option<IndexSettingsDocument>,
    lookup_node: &Lookup<NodeDocument>,
) -> Result<Vec<ShardStatsDocument>, eyre::Report> {
    let shard_docs: Vec<ShardStatsDocument> = shards
        .drain()
        .flat_map(|(number, mut shard_stats)| {
            shard_stats
                .drain(..)
                .filter_map(
                    |shard_entry| match EnrichedShardStats::try_from(shard_entry) {
                        Ok(stats) => {
                            let enriched_stats = stats.with_id(number);
                            let node = lookup_node.by_id(&enriched_stats.routing.node).cloned();
                            Some(
                                ShardStatsDocument::new(enriched_stats, shard_metadata.clone())
                                    .index_settings(index_settings.clone())
                                    .node(node)
                                    .calculate(),
                            )
                        }
                        Err(err) => {
                            log::warn!(
                                "Failed to parse shard stats for index {}, shard {}: {}",
                                &index_name,
                                number,
                                err
                            );
                            None
                        }
                    },
                )
                .collect::<Vec<ShardStatsDocument>>()
        })
        .collect();

    Ok(shard_docs)
}

#[skip_serializing_none]
#[derive(Serialize)]
struct IndexStatsDocument {
    index: EnrichedIndexStatsWithSettings,
    #[serde(flatten)]
    metadata: MetadataDoc,
}

impl IndexStatsDocument {
    fn new(index_stats: EnrichedIndexStatsWithSettings, metadata: MetadataDoc) -> Self {
        IndexStatsDocument {
            index: index_stats,
            metadata,
        }
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
            Some(seconds) => Some(
                (stats.primaries.store.size_in_bytes as f64 * (86_400.0 / seconds as f64)) as u64,
            ),
        };

        stats.total.indexing.est_bytes_per_day = match stats.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => {
                Some((stats.total.store.size_in_bytes as f64 * (86_400.0 / seconds as f64)) as u64)
            }
        };

        stats.primaries.bulk.est_bytes_per_day = match stats.write_phase_sec {
            None => None,
            Some(0) => Some(0),
            Some(seconds) => Some(
                (stats.primaries.bulk.total_size_in_bytes as f64 * (86_400.0 / seconds as f64))
                    as u64,
            ),
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
    pub alias: Option<Alias>,
    pub health: Option<String>,
    pub is_write_index: bool,
    pub primaries: EnrichedStats,
    pub since_rollover: Option<u64>,
    pub name: Option<String>,
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

    fn name(self, name: String) -> Self {
        Self {
            name: Some(name),
            ..self
        }
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
        let primaries = EnrichedStats::try_from(stats.primaries)?;
        let total = EnrichedStats::try_from(stats.total)?;

        Ok(EnrichedIndexStats {
            alias: None,
            health: stats.health,
            is_write_index: false,
            name: None,
            primaries,
            since_rollover: None,
            total,
            uuid: stats.uuid,
            write_phase_sec: None,
        })
    }
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
struct EnrichedShardStats {
    pub commit: ShardCommit,
    pub number: u16,
    pub retention_leases: RetentionLeases,
    pub routing: ShardRouting,
    pub search_idle: Option<bool>,
    pub search_idle_time: Option<u64>,
    pub seq_no: SequenceNumber,
    pub shard_path: Option<ShardPath>,
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
struct ShardStatsDocument {
    shard: EnrichedShardStats,
    index: Option<IndexSettingsDocument>,
    node: Option<NodeDocument>,
    #[serde(flatten)]
    metadata: MetadataDoc,
}

impl ShardStatsDocument {
    pub fn new(shard: EnrichedShardStats, metadata: MetadataDoc) -> Self {
        ShardStatsDocument {
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
    dense_vector: Option<VectorCount>,
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
    sparse_vector: Option<VectorCount>,
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
    pub total_size_in_bytes: Option<u64>,
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
