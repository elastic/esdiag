// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use bytes::Bytes;
use esdiag::{
    data::Uri,
    processor::{DataSource, SourceContext},
    receiver::{MissingSource, Receiver},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Write},
};
use tempfile::{TempDir, tempdir};
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Deserialize)]
struct CandidateSource {
    marker: String,
}

impl DataSource for CandidateSource {
    fn name() -> String {
        "nodes".to_owned()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["nodes_stats"]
    }
}

#[derive(Debug, Deserialize)]
struct NoCandidateSource;

impl DataSource for NoCandidateSource {
    fn name() -> String {
        "no_candidate_source".to_owned()
    }

    fn candidate_source_file_paths(_ctx: &SourceContext) -> eyre::Result<Vec<String>> {
        Ok(vec![])
    }
}

#[derive(Debug, Deserialize)]
struct AddressSource {
    ip: String,
    host: String,
    transport_address: String,
    node_id: String,
    numeric_id: Value,
}

impl DataSource for AddressSource {
    fn name() -> String {
        "nodes".to_owned()
    }
}

#[derive(Debug, Deserialize)]
struct ManifestAddress(AddressSource);

impl DataSource for ManifestAddress {
    fn name() -> String {
        "diagnostic_manifest".to_owned()
    }

    fn candidate_source_file_paths(_ctx: &SourceContext) -> eyre::Result<Vec<String>> {
        Ok(vec!["diagnostic_manifest.json".to_owned()])
    }
}

#[derive(Debug, Deserialize)]
struct VersionAddress(AddressSource);

impl DataSource for VersionAddress {
    fn name() -> String {
        "version".to_owned()
    }
}

fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for &(path, body) in entries {
        writer
            .start_file(
                path,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("start archive entry");
        writer.write_all(body.as_bytes()).expect("write archive entry");
    }
    writer.finish().expect("finish archive").into_inner()
}

fn receivers(bytes: Vec<u8>) -> (TempDir, [Receiver; 2]) {
    let fixture = tempdir().expect("fixture tempdir");
    let path = fixture.path().join("synthetic-bundle.zip");
    fs::write(&path, &bytes).expect("write fixture archive");
    let receivers = [
        Receiver::try_from(Uri::File(path)).expect("file receiver"),
        Receiver::try_from(Bytes::from(bytes)).expect("memory receiver"),
    ];
    for receiver in &receivers {
        set_source_product(receiver);
    }
    (fixture, receivers)
}

fn set_source_product(receiver: &Receiver) {
    match receiver {
        Receiver::ArchiveFile(adapter) => adapter.set_source_product("elasticsearch").expect("source product"),
        Receiver::ArchiveBytes(adapter) => adapter.set_source_product("elasticsearch").expect("source product"),
        _ => unreachable!("fixture contains only archive receivers"),
    }
}

fn set_scrubbed(receiver: &mut Receiver, enabled: bool) {
    match receiver {
        Receiver::ArchiveFile(adapter) => adapter.set_scrubbed(enabled),
        Receiver::ArchiveBytes(adapter) => adapter.set_scrubbed(enabled),
        _ => unreachable!("fixture contains only archive receivers"),
    }
}

fn missing(error: &eyre::Report) -> &MissingSource {
    error.downcast_ref::<MissingSource>().expect("typed MissingSource")
}

#[tokio::test]
async fn typed_candidate_order_selects_first_valid_entry_for_both_archives() {
    let (_fixture, receivers) = receivers(archive(&[
        ("nodes.json", r#"{"marker":"first"}"#),
        ("nodes_stats.json", r#"{"marker":"second"}"#),
    ]));
    for receiver in receivers {
        assert_eq!(
            receiver.get::<CandidateSource>().await.expect("typed source").marker,
            "first"
        );
    }
}

#[tokio::test]
async fn typed_whitespace_candidate_falls_through_for_both_archives() {
    let (_fixture, receivers) = receivers(archive(&[
        ("nodes.json", " \t\r\n  "),
        ("nodes_stats.json", r#"{"marker":"second"}"#),
    ]));
    for receiver in receivers {
        assert_eq!(
            receiver.get::<CandidateSource>().await.expect("typed source").marker,
            "second"
        );
    }
}

#[tokio::test]
async fn typed_malformed_first_candidate_never_falls_through_for_both_archives() {
    for malformed in [r#"{"marker":"unfinished"#, r#"{"marker":invalid}"#] {
        let (_fixture, receivers) = receivers(archive(&[
            ("nodes.json", malformed),
            ("nodes_stats.json", r#"{"marker":"second"}"#),
        ]));
        for receiver in receivers {
            let error = receiver
                .get::<CandidateSource>()
                .await
                .expect_err("invalid first candidate must fail");
            assert!(
                error.downcast_ref::<MissingSource>().is_none(),
                "present malformed JSON must not be absent"
            );
            assert!(
                error
                    .chain()
                    .any(|cause| cause.downcast_ref::<serde_json::Error>().is_some()),
                "present malformed JSON must report a JSON parse failure: {error:?}"
            );
        }
    }
}

#[tokio::test]
async fn typed_absence_retains_last_candidate_reason_and_path_for_both_archives() {
    let (_fixture, receivers) = receivers(archive(&[("nodes.json", " \t"), ("unrelated.json", "{}")]));
    for receiver in receivers {
        let error = receiver
            .get::<CandidateSource>()
            .await
            .expect_err("second candidate absent");
        assert!(matches!(missing(&error), MissingSource::ArchiveEntry { path } if path == "nodes_stats.json"));
    }
    let (_fixture, receivers) = self::receivers(archive(&[("unrelated.json", "{}"), ("nodes_stats.json", " \n\t")]));
    for receiver in receivers {
        let error = receiver
            .get::<CandidateSource>()
            .await
            .expect_err("empty last candidate");
        assert!(matches!(missing(&error), MissingSource::Empty { path } if path == "nodes_stats.json"));
    }

    let (_fixture, receivers) = self::receivers(archive(&[("unrelated.json", "{}")]));
    for receiver in receivers {
        let error = receiver
            .get::<NoCandidateSource>()
            .await
            .expect_err("no candidate paths");
        assert!(matches!(missing(&error), MissingSource::NoCandidates { source } if source == "no_candidate_source"));
    }
}

#[tokio::test]
async fn typed_nested_working_directory_and_legacy_double_slash_for_both_archives() {
    for nested_path in [
        "bundle/namespace/elasticsearch/cluster-one/nodes.json",
        "bundle/namespace/elasticsearch/cluster-one//nodes.json",
    ] {
        let (_fixture, mut receivers) =
            receivers(archive(&[("bundle/version.json", "{}"), (nested_path, ADDRESS_JSON)]));
        for receiver in &mut receivers {
            set_scrubbed(receiver, true);
            let nested = receiver
                .clone_for_subdir("namespace/elasticsearch/cluster-one")
                .expect("nested receiver");
            set_source_product(&nested);
            let node = nested.get::<AddressSource>().await.expect("typed nested source");
            assert_eq!(node.ip, "2.3.4.5");
            assert_eq!(node.node_id, "512.768.1024.1280");
        }
    }
}

const ADDRESS_JSON: &str = r#"{"ip":"512.768.1024.1280","host":"512.768.1024.1280","transport_address":"512.768.1024.1280:9300","node_id":"512.768.1024.1280","numeric_id":42}"#;

#[tokio::test]
async fn typed_scrub_allowlist_and_explicit_disable_for_both_archives() {
    let (_fixture, mut receivers) = receivers(archive(&[("nodes.json", ADDRESS_JSON)]));
    for receiver in &mut receivers {
        set_scrubbed(receiver, true);
        let node = receiver.get::<AddressSource>().await.expect("scrubbed typed source");
        assert_eq!(node.ip, "2.3.4.5");
        assert_eq!(node.host, "2.3.4.5");
        assert_eq!(node.transport_address, "2.3.4.5:9300");
        assert_eq!(node.node_id, "512.768.1024.1280");
        assert_eq!(node.numeric_id, json!(42));

        set_scrubbed(receiver, false);
        let original = receiver.get::<AddressSource>().await.expect("unscrubbed typed source");
        assert_eq!(original.ip, "512.768.1024.1280");
        assert_eq!(original.host, "512.768.1024.1280");
        assert_eq!(original.transport_address, "512.768.1024.1280:9300");
        assert_eq!(original.node_id, "512.768.1024.1280");
        assert_eq!(original.numeric_id, json!(42));
    }
}

#[tokio::test]
async fn typed_manifest_and_version_are_not_normalized_for_both_archives() {
    let (_fixture, mut receivers) = receivers(archive(&[
        ("diagnostic_manifest.json", ADDRESS_JSON),
        ("version.json", ADDRESS_JSON),
    ]));
    for receiver in &mut receivers {
        set_scrubbed(receiver, true);
        let manifest = receiver.get::<ManifestAddress>().await.expect("typed manifest");
        let version = receiver.get::<VersionAddress>().await.expect("typed version");
        for node in [&manifest.0, &version.0] {
            assert_eq!(node.ip, "512.768.1024.1280");
            assert_eq!(node.host, "512.768.1024.1280");
            assert_eq!(node.transport_address, "512.768.1024.1280:9300");
            assert_eq!(node.node_id, "512.768.1024.1280");
            assert_eq!(node.numeric_id, json!(42));
        }
    }
}

#[tokio::test]
async fn typed_corrupt_entry_read_is_not_reported_as_missing_for_both_archives() {
    let mut bytes = archive(&[("nodes.json", r#"{"marker":"alpha"}"#)]);
    let offset = bytes
        .windows(5)
        .position(|window| window == b"alpha")
        .expect("stored entry body");
    bytes[offset] = b'o'; // Valid JSON, but the stored ZIP entry's CRC now disagrees.
    let (_fixture, receivers) = receivers(bytes);
    for receiver in receivers {
        let error = receiver
            .get::<CandidateSource>()
            .await
            .expect_err("entry read must fail");
        assert!(
            error.downcast_ref::<MissingSource>().is_none(),
            "corrupt entry is not an absent source"
        );
        assert!(
            error.chain().any(|cause| {
                cause.downcast_ref::<std::io::Error>().is_some()
                    || cause
                        .downcast_ref::<serde_json::Error>()
                        .is_some_and(serde_json::Error::is_io)
            }),
            "corrupt entry must produce a read failure: {error:?}"
        );
    }
}
