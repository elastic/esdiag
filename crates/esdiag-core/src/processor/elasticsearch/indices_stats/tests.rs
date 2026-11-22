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
