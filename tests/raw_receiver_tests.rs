// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use bytes::Bytes;
use esdiag::{data::Uri, processor::DataSource, receiver::Receiver};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Write},
};
use tempfile::{TempDir, tempdir};
use zip::{ZipWriter, write::SimpleFileOptions};

const NODES_JSON: &str = r#"{
  "nodes": {
    "512.768.1024.1280": {
      "name": "512.768.1024.1280",
      "ip": "512.768.1024.1280",
      "host": "512.768.1024.1280",
      "transport_address": "512.768.1024.1280:9300",
      "label": "synthetic café"
    }
  }
}
"#;

struct NodesSource;

impl DataSource for NodesSource {
    fn name() -> String {
        "nodes".to_string()
    }
}

async fn local_raw_receivers(scrubbed: bool) -> (TempDir, [Receiver; 3]) {
    let fixture = tempdir().expect("fixture tempdir");
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (filename, content) in [
        (
            "diagnostic_manifest.json",
            r#"{"product":"elasticsearch","timestamp":"2026-01-01T00:00:00Z"}"#,
        ),
        ("nodes.json", NODES_JSON),
    ] {
        fs::write(fixture.path().join(filename), content).expect("write fixture file");
        archive
            .start_file(filename, SimpleFileOptions::default())
            .expect("start zip entry");
        archive.write_all(content.as_bytes()).expect("write zip entry");
    }
    let archive_bytes = Bytes::from(archive.finish().expect("finish archive").into_inner());
    let archive_path = fixture.path().join("synthetic-raw.zip");
    fs::write(&archive_path, &archive_bytes).expect("write archive");

    let mut bytes_receiver = Receiver::try_from(archive_bytes).expect("archive bytes receiver");
    if let Receiver::ArchiveBytes(receiver) = &mut bytes_receiver {
        receiver.set_scrubbed(scrubbed);
    }
    let receivers = [
        Receiver::try_from_with_scrub(Uri::File(archive_path), Some(scrubbed), None).expect("archive file receiver"),
        bytes_receiver,
        Receiver::try_from_with_scrub(Uri::Directory(fixture.path().to_path_buf()), Some(scrubbed), None)
            .expect("directory receiver"),
    ];
    for receiver in &receivers {
        receiver.try_get_manifest().await.expect("initialize source product");
    }
    (fixture, receivers)
}

#[tokio::test]
async fn local_raw_apis_normalize_only_allowlisted_addresses_when_scrubbed() {
    let (_fixture, receivers) = local_raw_receivers(true).await;
    let expected = json!({
        "nodes": {
            "512.768.1024.1280": {
                "name": "512.768.1024.1280",
                "ip": "2.3.4.5",
                "host": "2.3.4.5",
                "transport_address": "2.3.4.5:9300",
                "label": "synthetic café"
            }
        }
    });
    for receiver in receivers {
        let response = receiver.get_raw_response::<NodesSource>().await.expect("raw response");
        assert_eq!(serde_json::from_str::<Value>(&response.body).expect("JSON"), expected);
        assert_eq!(response.response_size_bytes, response.body.len() as u64);

        let raw = receiver.get_raw::<NodesSource>().await.expect("raw body");
        assert_eq!(serde_json::from_str::<Value>(&raw).expect("JSON"), expected);
    }
}

#[tokio::test]
async fn local_raw_apis_preserve_exact_content_when_scrub_disabled() {
    let (_fixture, receivers) = local_raw_receivers(false).await;
    for receiver in receivers {
        let response = receiver.get_raw_response::<NodesSource>().await.expect("raw response");
        assert_eq!(response.body, NODES_JSON);
        assert_eq!(response.response_size_bytes, NODES_JSON.len() as u64);
        assert_eq!(receiver.get_raw::<NodesSource>().await.expect("raw body"), NODES_JSON);
    }
}
