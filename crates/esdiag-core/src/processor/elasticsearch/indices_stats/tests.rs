use super::processor::{IndexStatsDocument, ShardStatsDocument};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::test]
async fn deserialize_shard_documents_is_ok() {
    let file = File::open("src/processor/elasticsearch/indices_stats/tests/metrics-shard-esdiag.ndjson")
        .await
        .unwrap();
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut documents = Vec::new();

    while let Some(line) = lines.next_line().await.unwrap() {
        if !line.trim().is_empty() {
            let doc: ShardStatsDocument = serde_json::from_str(&line).unwrap();
            documents.push(doc);
        }
    }

    let result = Ok::<Vec<ShardStatsDocument>, std::io::Error>(documents);
    assert!(result.is_ok());
}

#[tokio::test]
async fn deserialize_index_documents_is_ok() {
    let file = File::open("src/processor/elasticsearch/indices_stats/tests/metrics-index-esdiag.ndjson")
        .await
        .unwrap();
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut documents = Vec::new();

    while let Some(line) = lines.next_line().await.unwrap() {
        if !line.trim().is_empty() {
            let doc: IndexStatsDocument = serde_json::from_str(&line).unwrap();
            documents.push(doc);
        }
    }

    let result = Ok::<Vec<IndexStatsDocument>, std::io::Error>(documents);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_streaming_deserialization() {
    use crate::processor::diagnostic::data_source::StreamingDataSource;
    use crate::processor::elasticsearch::indices_stats::IndicesStats;
    use tokio::sync::mpsc;

    let json = r#"{
        "_shards": { "total": 1, "successful": 1, "failed": 0 },
        "indices": {
            "index1": { 
                "primaries": { "shard_stats": { "total_count": 1 } }, 
                "total": { "shard_stats": { "total_count": 1 } } 
            },
            "index2": { 
                "primaries": { "shard_stats": { "total_count": 1 } }, 
                "total": { "shard_stats": { "total_count": 1 } } 
            }
        }
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(10);

    let handle = tokio::task::spawn_blocking(move || {
        IndicesStats::deserialize_stream(&mut deserializer, tx).unwrap();
    });

    let mut count = 0;
    while let Some(res) = rx.recv().await {
        assert!(res.is_ok());
        count += 1;
    }
    assert_eq!(count, 2);
    handle.await.unwrap();
}

#[tokio::test]
async fn test_streaming_deserialization_handles_receiver_closed_mid_stream() {
    use crate::processor::diagnostic::data_source::StreamingDataSource;
    use crate::processor::elasticsearch::indices_stats::IndicesStats;
    use tokio::sync::mpsc;

    let json = r#"{
        "_shards": { "total": 1, "successful": 1, "failed": 0 },
        "indices": {
            "index1": {
                "primaries": { "shard_stats": { "total_count": 1 } },
                "total": { "shard_stats": { "total_count": 1 } }
            },
            "index2": {
                "primaries": { "shard_stats": { "total_count": 1 } },
                "total": { "shard_stats": { "total_count": 1 } }
            },
            "index3": {
                "primaries": { "shard_stats": { "total_count": 1 } },
                "total": { "shard_stats": { "total_count": 1 } }
            }
        }
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(1);

    let handle = tokio::task::spawn_blocking(move || {
        IndicesStats::deserialize_stream(&mut deserializer, tx).unwrap();
    });

    // Receive one streamed item, then close the receiver to simulate downstream shutdown.
    let _ = rx.recv().await;
    drop(rx);

    // Deserialization should still finish cleanly without trailing-comma parse failures.
    handle.await.unwrap();
}

#[test]
fn test_shard_entry_deserializes_from_reader_value_and_escaped_keys() {
    use crate::processor::elasticsearch::indices_stats::data::ShardEntry;

    let json = r#"{
        "rou\u0074ing": {"state": "STARTED", "primary": true, "node": "n1"},
        "commit": {"id": "c1", "generation": 1, "num_docs": 10},
        "seq_no": {"max_seq_no": 1, "local_checkpoint": 1, "global_checkpoint": 1},
        "retention_leases": {"primary_term": 1, "version": 1},
        "shard_stats": {"total_count": 1},
        "search_idle": false
    }"#;

    let from_reader: ShardEntry = serde_json::from_reader(std::io::Cursor::new(json.as_bytes()))
        .expect("ShardEntry should deserialize from reader");
    assert_eq!(from_reader.search_idle, Some(false));

    let value: serde_json::Value = serde_json::from_str(json).expect("valid json");
    let from_value: ShardEntry = serde_json::from_value(value).expect("ShardEntry should deserialize from owned Value");
    assert_eq!(from_value.search_idle, Some(false));
}

#[test]
fn shard_avg_docs_sec_uses_persistent_docs_count_when_index_total_is_zero() {
    use super::processor::EnrichedShardStats;
    use crate::processor::elasticsearch::{
        indices_settings::{IndexSettings, IndexSettingsDocument},
        indices_stats::ShardEntry,
        metadata::MetadataValue,
    };
    use serde_json::json;
    use std::sync::Arc;

    let shard_entry: ShardEntry = serde_json::from_value(json!({
        "routing": {
            "state": "STARTED",
            "primary": true,
            "node": "node-1"
        },
        "docs": { "count": 50_000, "deleted": 0 },
        "store": { "size_in_bytes": 50_000_000 },
        "indexing": {
            "index_total": 0,
            "index_time_in_millis": 0,
            "index_current": 0,
            "index_failed": 0,
            "delete_total": 0,
            "delete_time_in_millis": 0,
            "delete_current": 0
        },
        "shard_stats": { "total_count": 1 },
        "commit": { "id": "c1", "generation": 1, "user_data": {}, "num_docs": 50_000 },
        "seq_no": { "max_seq_no": 50_000, "local_checkpoint": 50_000, "global_checkpoint": 50_000 },
        "retention_leases": { "primary_term": 1, "version": 1, "leases": [] }
    }))
    .unwrap();

    let raw_meta = MetadataValue(Arc::new(json!({})));
    let settings: IndexSettings = serde_json::from_value(json!({
        "uuid": "idx-uuid-1",
        "creation_date": "1700000000000",
        "number_of_shards": "1",
        "number_of_replicas": "1"
    }))
    .unwrap();
    let index_settings = IndexSettingsDocument::from(settings.name("logs-000001".to_string()).build());

    let enriched = EnrichedShardStats::try_from(shard_entry).unwrap().with_id(0);
    let doc = ShardStatsDocument::new(enriched, raw_meta.clone())
        .index_settings(Some(index_settings.clone().write_phase(Some(500))))
        .calculate();
    let value = serde_json::to_value(&doc).unwrap();
    assert_eq!(value["shard"]["indexing"]["avg_docs_sec"], 100);
    assert_eq!(value["shard"]["indexing"]["avg_bytes_sec"], 100_000);

    // Zero write_phase_sec must still yield 0 without dividing by zero.
    let zero_doc = ShardStatsDocument::new(
        EnrichedShardStats::try_from(
            serde_json::from_value::<ShardEntry>(json!({
                "routing": { "state": "STARTED", "primary": true, "node": "node-1" },
                "docs": { "count": 50_000, "deleted": 0 },
                "store": { "size_in_bytes": 50_000_000 },
                "indexing": {
                    "index_total": 0,
                    "index_time_in_millis": 0,
                    "index_current": 0,
                    "index_failed": 0,
                    "delete_total": 0,
                    "delete_time_in_millis": 0,
                    "delete_current": 0
                },
                "shard_stats": { "total_count": 1 },
                "commit": { "id": "c1", "generation": 1, "user_data": {}, "num_docs": 50_000 },
                "seq_no": { "max_seq_no": 50_000, "local_checkpoint": 50_000, "global_checkpoint": 50_000 },
                "retention_leases": { "primary_term": 1, "version": 1, "leases": [] }
            }))
            .unwrap(),
        )
        .unwrap()
        .with_id(0),
        raw_meta,
    )
    .index_settings(Some(index_settings.write_phase(Some(0))))
    .calculate();
    let zero_value = serde_json::to_value(&zero_doc).unwrap();
    assert_eq!(zero_value["shard"]["indexing"]["avg_docs_sec"], 0);
}

#[tokio::test]
async fn process_index_propagates_alias_write_index_and_persistent_doc_rate_to_shards() {
    use super::processor::{IndexProcessingContext, process_index};
    use crate::data::{Application, Platform};
    use crate::processor::diagnostic::DiagnosticMetadata;
    use crate::processor::elasticsearch::{
        Alias, ElasticsearchMetadata, IlmStats, Lookup, Lookups, indices_settings::IndexSettings,
        indices_stats::IndexStats, metadata::MetadataValue, version::ClusterMetadata,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let index_name = "app-logs-000001".to_string();
    let creation_date_ms: u64 = 1_700_000_000_000;
    let collection_date_ms: u64 = creation_date_ms + 500_000; // 500s write phase

    let mut alias_lookup = Lookup::<Alias>::new();
    let alias: Alias = serde_json::from_value(json!({
        "name": "app-logs",
        "is_write_index": true,
        "is_hidden": false
    }))
    .unwrap();
    alias_lookup.add(alias).with_name(&index_name);

    let mut ilm_lookup = Lookup::<IlmStats>::new();
    let ilm: IlmStats = serde_json::from_value(json!({
        "managed": true,
        "action": "rollover",
        "lifecycle_date_millis": creation_date_ms
    }))
    .unwrap();
    ilm_lookup.add(ilm).with_name(&index_name);

    let mut index_settings_lookup = Lookup::<IndexSettings>::new();
    let settings: IndexSettings = serde_json::from_value(json!({
        "uuid": "idx-uuid-alias",
        "creation_date": creation_date_ms.to_string(),
        "number_of_shards": "1",
        "number_of_replicas": "1"
    }))
    .unwrap();
    index_settings_lookup
        .add(settings.name(index_name.clone()).build())
        .with_name(&index_name);

    let lookups = Lookups {
        alias: alias_lookup,
        data_stream: Lookup::new(),
        ilm_explain: ilm_lookup,
        index_settings: index_settings_lookup,
        mapping_stats: Lookup::new(),
        node: Lookup::new(),
        shared_cache: Lookup::new(),
    };

    let raw_meta = MetadataValue(Arc::new(json!({})));
    let metadata = ElasticsearchMetadata {
        cluster: serde_json::from_value::<ClusterMetadata>(json!({
            "display_name": "test-cluster",
            "diagnostic_node": "node-1",
            "name": "test-cluster",
            "uuid": "cluster-uuid",
            "version": {
                "number": "8.17.0",
                "build_flavor": "default",
                "build_type": "tar",
                "build_hash": "0000000",
                "build_date": "2026-01-01T00:00:00Z",
                "build_snapshot": false,
                "lucene_version": "9.12.0",
                "minimum_wire_compatibility_version": "7.17.0",
                "minimum_index_compatibility_version": "7.0.0"
            }
        }))
        .unwrap(),
        diagnostic: DiagnosticMetadata::new(
            collection_date_ms,
            "test-cluster@2026-01-01~0000".to_string(),
            "test".to_string(),
            None,
            "diag-uuid".to_string(),
        ),
        platform: Platform::SelfManaged,
        application: Some(Application::Elasticsearch),
        timestamp: collection_date_ms,
        as_doc: raw_meta.clone(),
    };

    let index_stats: IndexStats = serde_json::from_value(json!({
        "uuid": "idx-uuid-alias",
        "health": "green",
        "primaries": {
            "docs": { "count": 50_000, "deleted": 0 },
            "store": { "size_in_bytes": 50_000_000 },
            "indexing": {
                "index_total": 0,
                "index_time_in_millis": 0,
                "index_current": 0,
                "index_failed": 0,
                "delete_total": 0,
                "delete_time_in_millis": 0,
                "delete_current": 0
            },
            "shard_stats": { "total_count": 1 }
        },
        "total": {
            "docs": { "count": 50_000, "deleted": 0 },
            "store": { "size_in_bytes": 50_000_000 },
            "indexing": {
                "index_total": 0,
                "index_time_in_millis": 0,
                "index_current": 0,
                "index_failed": 0,
                "delete_total": 0,
                "delete_time_in_millis": 0,
                "delete_current": 0
            },
            "shard_stats": { "total_count": 1 }
        },
        "shards": {
            "0": [{
                "routing": { "state": "STARTED", "primary": true, "node": "node-1" },
                "docs": { "count": 50_000, "deleted": 0 },
                "store": { "size_in_bytes": 50_000_000 },
                "indexing": {
                    "index_total": 0,
                    "index_time_in_millis": 0,
                    "index_current": 0,
                    "index_failed": 0,
                    "delete_total": 0,
                    "delete_time_in_millis": 0,
                    "delete_current": 0
                },
                "shard_stats": { "total_count": 1 },
                "commit": { "id": "c1", "generation": 1, "user_data": {}, "num_docs": 50_000 },
                "seq_no": { "max_seq_no": 50_000, "local_checkpoint": 50_000, "global_checkpoint": 50_000 },
                "retention_leases": { "primary_term": 1, "version": 1, "leases": [] }
            }]
        }
    }))
    .unwrap();

    let (index_tx, mut index_rx) = mpsc::channel(4);
    let (shard_tx, mut shard_rx) = mpsc::channel(4);
    let ctx = IndexProcessingContext {
        lookups: &lookups,
        metadata: &metadata,
        index_metadata: &raw_meta,
        shard_metadata: &raw_meta,
        index_tx: &index_tx,
        shard_tx: &shard_tx,
    };

    process_index(index_name, index_stats, &ctx).await;

    let index_doc = serde_json::to_value(index_rx.recv().await.unwrap()).unwrap();
    let shard_doc = serde_json::to_value(shard_rx.recv().await.unwrap()).unwrap();

    assert_eq!(index_doc["index"]["is_write_index"], true);
    assert_eq!(index_doc["index"]["write_phase_sec"], 500);
    assert_eq!(shard_doc["index"]["is_write_index"], true);
    assert_eq!(shard_doc["index"]["write_phase_sec"], 500);
    assert_eq!(shard_doc["shard"]["indexing"]["avg_docs_sec"], 100);
    assert_eq!(shard_doc["shard"]["indexing"]["avg_bytes_sec"], 100_000);
}
