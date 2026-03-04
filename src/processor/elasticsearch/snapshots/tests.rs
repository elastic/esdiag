use crate::processor::StreamingDataSource;
use crate::processor::elasticsearch::snapshots::{Snapshots, extract_snapshot_date};
use tokio::sync::mpsc;

#[test]
fn extract_snapshot_date_present() {
    assert_eq!(
        extract_snapshot_date("snapshot-2026.03.03-prod"),
        Some("2026-03-03".to_string())
    );
}

#[test]
fn extract_snapshot_date_absent() {
    assert_eq!(extract_snapshot_date("snapshot-prod"), None);
}

#[tokio::test]
async fn test_streaming_deserialization() {
    let json = r#"{
        "snapshots": [
            {
                "snapshot": "daily-2026.03.01",
                "repository": "repo-a",
                "state": "SUCCESS",
                "indices": ["idx-a"]
            },
            {
                "snapshot": "daily-2026.03.02",
                "repository": "repo-b",
                "state": "FAILED",
                "indices": ["idx-b"]
            }
        ]
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(10);
    let handle = tokio::task::spawn_blocking(move || {
        Snapshots::deserialize_stream(&mut deserializer, tx).expect("stream deserialize")
    });

    let mut count = 0;
    while let Some(res) = rx.recv().await {
        assert!(res.is_ok());
        count += 1;
    }
    assert_eq!(count, 2);
    handle.await.expect("join");
}

#[tokio::test]
async fn test_streaming_deserialization_handles_receiver_closed_mid_stream() {
    let json = r#"{
        "snapshots": [
            {"snapshot": "s-2026.03.01", "state": "SUCCESS"},
            {"snapshot": "s-2026.03.02", "state": "SUCCESS"},
            {"snapshot": "s-2026.03.03", "state": "SUCCESS"}
        ]
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(1);
    let handle = tokio::task::spawn_blocking(move || {
        Snapshots::deserialize_stream(&mut deserializer, tx).expect("stream deserialize")
    });

    let _ = rx.recv().await;
    drop(rx);
    handle.await.expect("join");
}

#[tokio::test]
async fn test_streaming_deserialization_continues_on_entry_parse_error() {
    let json = r#"{
        "snapshots": [
            {"snapshot": "s-2026.03.01", "state": "SUCCESS"},
            {"snapshot": 42, "state": "SUCCESS"},
            {"snapshot": "s-2026.03.03", "state": "SUCCESS"}
        ]
    }"#;

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let (tx, mut rx) = mpsc::channel(10);
    let handle = tokio::task::spawn_blocking(move || {
        Snapshots::deserialize_stream(&mut deserializer, tx).expect("stream deserialize")
    });

    let mut ok_count = 0;
    let mut err_count = 0;
    while let Some(res) = rx.recv().await {
        match res {
            Ok(_) => ok_count += 1,
            Err(_) => err_count += 1,
        }
    }

    assert_eq!(ok_count, 2);
    assert_eq!(err_count, 1);
    handle.await.expect("join");
}

#[tokio::test]
async fn test_streaming_deserialization_large_payload() {
    let mut snapshot_items = String::new();
    for i in 0..5000 {
        if i > 0 {
            snapshot_items.push(',');
        }
        snapshot_items.push_str(&format!(
            r#"{{"snapshot":"daily-2026.03.{:02}","repository":"repo-a","state":"SUCCESS"}}"#,
            (i % 28) + 1
        ));
    }
    let json = format!(r#"{{"snapshots":[{}]}}"#, snapshot_items);

    let (tx, mut rx) = mpsc::channel(128);
    let handle = tokio::task::spawn_blocking(move || {
        let mut deserializer = serde_json::Deserializer::from_str(&json);
        Snapshots::deserialize_stream(&mut deserializer, tx).expect("stream deserialize")
    });

    let mut count = 0;
    while let Some(res) = rx.recv().await {
        assert!(res.is_ok());
        count += 1;
    }
    assert_eq!(count, 5000);
    handle.await.expect("join");
}
