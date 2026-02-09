#[tokio::test]
async fn test_streaming_deserialization() {
    use crate::processor::diagnostic::data_source::StreamingDataSource;
    use crate::processor::elasticsearch::nodes_stats::NodesStats;
    use tokio::sync::mpsc;

    let json = r#"{
        "_nodes": { "total": 1, "successful": 1, "failed": 0 },
        "nodes": {
            "node1": { 
                "name": "node-1",
                "transport_address": "127.0.0.1:9300",
                "host": "127.0.0.1",
                "ip": "127.0.0.1",
                "roles": ["master", "data"],
                "attributes": {},
                "http": {},
                "transport": {},
                "discovery": {},
                "ingest": { "total": {} },
                "thread_pool": {},
                "jvm": {},
                "fs": { "total": { "total_in_bytes": 100, "free_in_bytes": 50, "available_in_bytes": 50 } },
                "os": { "timestamp": 0, "cpu": { "percent": 0, "load_average": { "1m": 0.0, "5m": 0.0, "15m": 0.0 } }, "mem": {} },
                "process": {},
                "script": {},
                "script_cache": {},
                "indexing_pressure": {},
                "indices": {},
                "breakers": {}
            }
        }
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(10);

    let handle = tokio::task::spawn_blocking(move || {
        NodesStats::deserialize_stream(&mut deserializer, tx).unwrap();
    });

    let mut count = 0;
    while let Some(res) = rx.recv().await {
        let _ = res.unwrap_or_else(|e| panic!("Error: {}", e));
        count += 1;
    }
    assert_eq!(count, 1);
    handle.await.unwrap();
}
