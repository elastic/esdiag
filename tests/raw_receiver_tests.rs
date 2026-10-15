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

struct VersionSource;
impl DataSource for VersionSource {
    fn name() -> String {
        "version".into()
    }
    fn candidate_source_file_paths(_: &esdiag::processor::SourceContext) -> eyre::Result<Vec<String>> {
        Ok(vec!["version.json".into()])
    }
}

struct TextSource;
impl DataSource for TextSource {
    fn name() -> String {
        "diagnostic".into()
    }
    fn candidate_source_file_paths(_: &esdiag::processor::SourceContext) -> eyre::Result<Vec<String>> {
        Ok(vec!["diagnostic.txt".into()])
    }
}

struct ManifestSource;
impl DataSource for ManifestSource {
    fn name() -> String {
        "diagnostic_manifest".into()
    }
    fn candidate_source_file_paths(_: &esdiag::processor::SourceContext) -> eyre::Result<Vec<String>> {
        Ok(vec!["diagnostic_manifest.json".into()])
    }
}

async fn local_raw_receivers(scrubbed: bool) -> (TempDir, [Receiver; 3]) {
    local_raw_receivers_with_content(scrubbed, NODES_JSON).await
}

async fn local_raw_receivers_with_content(scrubbed: bool, nodes_json: &str) -> (TempDir, [Receiver; 3]) {
    let fixture = tempdir().expect("fixture tempdir");
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (filename, content) in [
        (
            "diagnostic_manifest.json",
            r#"{"product":"elasticsearch","timestamp":"2026-01-01T00:00:00Z"}"#,
        ),
        ("nodes.json", nodes_json),
        ("version.json", " \nnot valid JSON, café\n"),
        ("diagnostic.txt", " \nnot valid JSON, café\n"),
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

#[tokio::test]
async fn supported_raw_reads_collapse_decoded_duplicate_keys_before_scrubbing() {
    let input = r#"{"ip":"512.768.1024.1280","\u0069p":"192.0.2.10","host":"512.768.1024.1280","host":"513.769.1025.1281:9300","nested":{"ip":"512.768.1024.1280","ip":"192.0.2.11"},"bound_address":["512.768.1024.1280:9300",{"ip":"512.768.1024.1280"}],"label":"café"}"#;
    let expected = json!({
        "ip": "192.0.2.10",
        "host": "3.4.5.6",
        "nested": {"ip": "192.0.2.11"},
        "bound_address": ["2.3.4.5:9300", {"ip": "2.3.4.5"}],
        "label": "café"
    });
    let (_fixture, receivers) = local_raw_receivers_with_content(true, input).await;
    for receiver in receivers {
        let response = receiver.get_raw_response::<NodesSource>().await.expect("raw response");
        assert_eq!(serde_json::from_str::<Value>(&response.body).unwrap(), expected);
        assert_eq!(response.response_size_bytes, response.body.len() as u64);
        assert_ne!(response.response_size_bytes, input.len() as u64);
        assert!(response.body.len() > response.body.chars().count());
        assert_eq!(
            serde_json::from_str::<Value>(&receiver.get_raw::<NodesSource>().await.unwrap()).unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn supported_raw_reads_preserve_value_numeric_types_and_boundaries() {
    let input = r#"{"lower":-9223372036854775808,"upper":9223372036854775807,"unsigned":18446744073709551615,"above_unsigned":18446744073709551616,"below_signed":-9223372036854775809,"exponent":1e2,"negative_zero":-0,"negative_float_zero":-0.0,"ip":"512.768.1024.1280"}"#;
    let (_fixture, receivers) = local_raw_receivers_with_content(true, input).await;
    for receiver in receivers {
        let response = receiver.get_raw_response::<NodesSource>().await.unwrap();
        assert_eq!(response.response_size_bytes, response.body.len() as u64);
        for body in [response.body, receiver.get_raw::<NodesSource>().await.unwrap()] {
            let value: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(value["ip"], "2.3.4.5");
            assert_eq!(value["lower"].as_i64(), Some(i64::MIN));
            assert_eq!(value["upper"].as_i64(), Some(i64::MAX));
            assert_eq!(value["unsigned"].as_u64(), Some(u64::MAX));
            assert!(value["above_unsigned"].is_f64());
            assert_eq!(value["above_unsigned"].as_f64(), Some(18446744073709551616.0));
            assert!(value["below_signed"].is_f64());
            assert_eq!(value["below_signed"].as_f64(), Some(-9223372036854775809.0));
            assert!(value["exponent"].is_f64());
            assert_eq!(value["exponent"].as_f64(), Some(100.0));
            assert!(value["negative_zero"].is_f64());
            assert!(value["negative_zero"].as_f64().unwrap().is_sign_negative());
            assert!(value["negative_float_zero"].is_f64());
            assert!(value["negative_float_zero"].as_f64().unwrap().is_sign_negative());
        }
    }
}

#[tokio::test]
async fn supported_raw_reads_reject_incomplete_invalid_and_out_of_range_json() {
    for input in [
        r#"{"ip":"512.768.1024.1280""#,
        r#"{"ip":"512.768.1024.1280}"#,
        r#"{"ip":true} trailing"#,
        r#"{"ip":tru}"#,
        r#"{"ip":01}"#,
        r#"{"number":1e400}"#,
    ] {
        let (_fixture, receivers) = local_raw_receivers_with_content(true, input).await;
        for receiver in receivers {
            assert!(receiver.get_raw::<NodesSource>().await.is_err(), "accepted {input}");
            assert!(
                receiver.get_raw_response::<NodesSource>().await.is_err(),
                "accepted response {input}"
            );
        }
    }
}

#[tokio::test]
async fn supported_raw_reads_keep_the_value_deserializer_nesting_limit() {
    for depth in [8, 127] {
        let shallow = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        let (_fixture, receivers) = local_raw_receivers_with_content(true, &shallow).await;
        for receiver in receivers {
            let response = receiver.get_raw_response::<NodesSource>().await.unwrap();
            assert_eq!(response.response_size_bytes, response.body.len() as u64);
            assert_eq!(
                serde_json::from_str::<Value>(&response.body).unwrap(),
                serde_json::from_str::<Value>(&shallow).unwrap()
            );
            assert!(receiver.get_raw::<NodesSource>().await.is_ok());
        }
    }
    for depth in [128, 129] {
        let deep = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        let (_fixture, receivers) = local_raw_receivers_with_content(true, &deep).await;
        for receiver in receivers {
            assert!(receiver.get_raw::<NodesSource>().await.is_err());
            assert!(receiver.get_raw_response::<NodesSource>().await.is_err());
        }
    }
}

#[tokio::test]
async fn raw_nesting_errors_precede_later_payload_syntax_errors() {
    for (open, close) in [("[", "]"), ("{\"child\":", "}")] {
        let input = format!(
            "{}\"{}\"{} trailing",
            open.repeat(128),
            "x".repeat(64 * 1024),
            close.repeat(128)
        );
        let (_fixture, receivers) = local_raw_receivers_with_content(true, &input).await;
        for receiver in receivers {
            let error = receiver
                .get_raw_response::<NodesSource>()
                .await
                .expect_err("excessive nesting must fail before the later payload error");
            let syntax = error
                .downcast_ref::<serde_json::Error>()
                .expect("raw validation retains the JSON syntax error");
            assert!(syntax.is_syntax());
            assert!(
                syntax.column() <= open.len() * 128,
                "validation traversed beyond the over-budget opening delimiter: {syntax}"
            );
        }
    }
}

#[tokio::test]
async fn disabled_and_unsupported_raw_reads_return_original_bytes_even_if_invalid_json() {
    let invalid = " \n{\"ip\":\"512.768.1024.1280\"} trailing café\n";
    let excluded_or_non_json = " \nnot valid JSON, café\n";
    for scrubbed in [false, true] {
        let (_fixture, receivers) = local_raw_receivers_with_content(scrubbed, invalid).await;
        for receiver in receivers {
            if !scrubbed {
                let response = receiver.get_raw_response::<NodesSource>().await.unwrap();
                assert_eq!(response.body, invalid);
                assert_eq!(response.response_size_bytes, invalid.len() as u64);
                assert_eq!(receiver.get_raw::<NodesSource>().await.unwrap(), invalid);
            }
            for (response, raw) in [
                (
                    receiver.get_raw_response::<VersionSource>().await.unwrap(),
                    receiver.get_raw::<VersionSource>().await.unwrap(),
                ),
                (
                    receiver.get_raw_response::<TextSource>().await.unwrap(),
                    receiver.get_raw::<TextSource>().await.unwrap(),
                ),
            ] {
                assert_eq!(response.body, excluded_or_non_json);
                assert_eq!(response.response_size_bytes, excluded_or_non_json.len() as u64);
                assert_eq!(raw, excluded_or_non_json);
            }
            let manifest = receiver.get_raw_response::<ManifestSource>().await.unwrap();
            assert_eq!(
                manifest.body,
                r#"{"product":"elasticsearch","timestamp":"2026-01-01T00:00:00Z"}"#
            );
            assert_eq!(manifest.response_size_bytes, manifest.body.len() as u64);
            assert_eq!(receiver.get_raw::<ManifestSource>().await.unwrap(), manifest.body);
        }
    }
}
