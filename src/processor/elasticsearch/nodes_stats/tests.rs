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

#[test]
fn test_node_stats_os_enrichment_from_lookup() {
    use crate::processor::elasticsearch::nodes::NodeDocument;
    use crate::processor::elasticsearch::nodes_stats::NodeStats;
    use serde_json::json;

    let mut node_stats: NodeStats = serde_json::from_value(json!({
        "name": "instance-0000000309",
        "transport_address": "127.0.0.1:9300",
        "host": "127.0.0.1",
        "ip": "127.0.0.1:9300",
        "roles": ["master", "data"],
        "attributes": { "raw": "stats" },
        "http": {},
        "transport": {},
        "discovery": {},
        "ingest": { "total": {} },
        "thread_pool": {},
        "jvm": {},
        "fs": { "total": { "total_in_bytes": 100, "free_in_bytes": 50, "available_in_bytes": 50 } },
        "os": {
            "timestamp": 123,
            "cpu": { "percent": 0, "load_average": { "1m": 0.0, "5m": 0.0, "15m": 0.0 } },
            "mem": {}
        },
        "process": {},
        "script": {},
        "script_cache": {},
        "indexing_pressure": {},
        "indices": {},
        "breakers": {}
    }))
    .expect("parse NodeStats");

    let lookup_node: NodeDocument = serde_json::from_value(json!({
        "attributes": {
            "instance_configuration": "aws.es.datahot.i3"
        },
        "name": "hot-309",
        "id": "node-id",
        "host": "10.89.0.2",
        "ip": "10.89.0.2",
        "role": "dm",
        "roles": ["data_hot", "ingest"],
        "tier": "hot",
        "tier_order": 2,
        "version": "9.1.3",
        "os": {
            "allocated_processors": 8,
            "arch": "aarch64",
            "available_processors": 8,
            "name": "Linux",
            "pretty_name": "Red Hat Enterprise Linux 9.6 (Plow)",
            "refresh_interval_in_millis": 1000,
            "version": "6.15.6-200.fc42.aarch64"
        }
    }))
    .expect("parse NodeDocument");

    node_stats.calculate_stats(8);
    node_stats.enrich_from_lookup(&lookup_node);

    let node_stats_json = serde_json::to_value(&node_stats).expect("serialize NodeStats");
    let os = &node_stats_json["os"];

    assert_eq!(node_stats_json["name"], "hot-309");
    assert_eq!(node_stats_json["host"], "10.89.0.2");
    assert_eq!(node_stats_json["ip"], "127.0.0.1:9300");
    assert_eq!(
        node_stats_json["attributes"]["instance_configuration"],
        "aws.es.datahot.i3"
    );
    assert_eq!(node_stats_json["attributes"].get("raw"), None);
    let roles = node_stats_json["roles"].as_array().expect("roles array");
    assert!(roles.iter().any(|role| role == "data_hot"));
    assert!(roles.iter().any(|role| role == "ingest"));
    assert!(!roles.iter().any(|role| role == "master"));
    assert_eq!(os["timestamp"], 123);
    assert_eq!(os["cpu"]["percent"], 0);
    assert_eq!(os["refresh_interval_in_millis"], 1000);
    assert_eq!(os["name"], "Linux");
    assert_eq!(os["pretty_name"], "Red Hat Enterprise Linux 9.6 (Plow)");
    assert_eq!(os["arch"], "aarch64");
    assert_eq!(os["version"], "6.15.6-200.fc42.aarch64");
    assert_eq!(os["available_processors"], 8);
    assert_eq!(os["allocated_processors"], 8);
}
