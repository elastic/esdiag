use super::processor::{IndexStatsDocument, ShardStatsDocument};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::test]
async fn deserialize_shard_documents_is_ok() {
    let file =
        File::open("src/processor/elasticsearch/indices_stats/tests/metrics-shard-esdiag.ndjson")
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
    let file =
        File::open("src/processor/elasticsearch/indices_stats/tests/metrics-index-esdiag.ndjson")
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

    tokio::task::spawn_blocking(move || {
        IndicesStats::deserialize_stream(&mut deserializer, tx).unwrap();
    });

    let mut count = 0;
    while let Some(res) = rx.recv().await {
        assert!(res.is_ok());
        count += 1;
    }
    assert_eq!(count, 2);
}
