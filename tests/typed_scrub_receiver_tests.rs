// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use bytes::Bytes;
use esdiag::{
    data::Uri,
    processor::{DataSource, SourceContext, StreamingDataSource},
    receiver::Receiver,
};
use eyre::Result;
use futures::StreamExt;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::{Value, value::RawValue};
use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{Cursor, Write},
};
use tempfile::{TempDir, tempdir};
use tokio::sync::mpsc::Sender;
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct TypedDocument(BTreeMap<String, Box<RawValue>>);

impl DataSource for TypedDocument {
    fn name() -> String {
        "nodes".into()
    }

    fn candidate_source_file_paths(_: &SourceContext) -> Result<Vec<String>> {
        Ok(vec!["nodes.json".into()])
    }

    fn resolve_source_file_path(_: &SourceContext) -> Result<String> {
        Ok("nodes.json".into())
    }
}

impl StreamingDataSource for TypedDocument {
    type Item = (String, Box<RawValue>);

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Entries(Sender<Result<(String, Box<RawValue>)>>);
        impl<'de> Visitor<'de> for Entries {
            type Value = ();

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object of raw diagnostic entries")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
                while let Some(entry) = map.next_entry::<String, Box<RawValue>>()? {
                    self.0.blocking_send(Ok(entry)).map_err(serde::de::Error::custom)?;
                }
                Ok(())
            }
        }
        deserializer.deserialize_map(Entries(sender))
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
struct DepthDocument(Value);

impl DataSource for DepthDocument {
    fn name() -> String {
        "nodes".into()
    }
    fn candidate_source_file_paths(_: &SourceContext) -> Result<Vec<String>> {
        Ok(vec!["nodes.json".into()])
    }
    fn resolve_source_file_path(_: &SourceContext) -> Result<String> {
        Ok("nodes.json".into())
    }
}

impl StreamingDataSource for DepthDocument {
    type Item = Value;

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        sender.blocking_send(Ok(value)).map_err(serde::de::Error::custom)
    }
}

async fn receivers_for(input: &str) -> (TempDir, [Receiver; 3]) {
    receivers_for_scrub(input, true).await
}

async fn receivers_for_scrub(input: &str, scrubbed: bool) -> (TempDir, [Receiver; 3]) {
    let fixture = tempdir().expect("fixture directory");
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in [
        (
            "diagnostic_manifest.json",
            r#"{"product":"elasticsearch","timestamp":"2026-01-01T00:00:00Z"}"#,
        ),
        ("nodes.json", input),
    ] {
        fs::write(fixture.path().join(name), body).expect("write fixture");
        zip.start_file(name, SimpleFileOptions::default()).expect("zip entry");
        zip.write_all(body.as_bytes()).expect("zip content");
    }
    let zip_bytes = Bytes::from(zip.finish().expect("finish zip").into_inner());
    let path = fixture.path().join("synthetic.zip");
    fs::write(&path, &zip_bytes).expect("write archive");
    let mut bytes = Receiver::try_from(zip_bytes).expect("archive bytes");
    if let Receiver::ArchiveBytes(receiver) = &mut bytes {
        receiver.set_scrubbed(scrubbed);
    }
    let receivers = [
        Receiver::try_from_with_scrub(Uri::File(path), Some(scrubbed), None).expect("archive file"),
        bytes,
        Receiver::try_from_with_scrub(Uri::Directory(fixture.path().to_path_buf()), Some(scrubbed), None)
            .expect("directory"),
    ];
    for receiver in &receivers {
        receiver.try_get_manifest().await.expect("source product");
    }
    (fixture, receivers)
}

#[tokio::test]
async fn eager_and_streaming_consumers_keep_lexemes_while_scrubbing_decoded_field_scope() {
    let input = r#"{"\u0069p":["512.768.1024.1280:9300",["513.769.1025.1281",{"name":"512.768.1024.1280","host":"512.768.1024.1280:9300"}]],"host":{"name":"512.768.1024.1280","ip":"513.769.1025.1281"},"nested":{"lab\u0065l":"caf\u00e9","ip":"512.768.1024.1280","other":"漢字"},"label":"caf\u00e9","unicode":"漢字","huge":1e9999,"number":1.2300e+08,"negative_zero":-0,"lower":-9223372036854775808,"unsigned":18446744073709551615,"overflow":18446744073709551616}"#;
    let (_fixture, receivers) = receivers_for(input).await;
    for receiver in receivers {
        let eager = receiver.get::<TypedDocument>().await.expect("eager document");
        assert_eq!(
            serde_json::from_str::<Value>(eager.0["ip"].get()).unwrap(),
            serde_json::json!([
                "2.3.4.5", ["3.4.5.6", {"name":"512.768.1024.1280", "host":"2.3.4.5"}]
            ])
        );
        assert_eq!(
            serde_json::from_str::<Value>(eager.0["host"].get()).unwrap(),
            serde_json::json!({"name":"512.768.1024.1280", "ip":"3.4.5.6"})
        );
        assert_eq!(
            eager.0["nested"].get(),
            r#"{"lab\u0065l":"caf\u00e9","ip":"2.3.4.5","other":"漢字"}"#
        );
        assert_eq!(eager.0["label"].get(), r#""caf\u00e9""#);
        assert_eq!(eager.0["unicode"].get(), r#""漢字""#);
        assert_eq!(eager.0["huge"].get(), "1e9999");
        assert_eq!(eager.0["number"].get(), "1.2300e+08");
        assert_eq!(eager.0["negative_zero"].get(), "-0");
        assert_eq!(eager.0["lower"].get(), "-9223372036854775808");
        assert_eq!(eager.0["unsigned"].get(), "18446744073709551615");
        assert_eq!(eager.0["overflow"].get(), "18446744073709551616");

        let stream = receiver.get_stream::<TypedDocument>().await.expect("stream");
        drop(receiver);
        let items = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .expect("stream entries");
        let streamed: BTreeMap<_, _> = items.into_iter().collect();
        assert_eq!(streamed.len(), eager.0.len());
        for (key, value) in &eager.0 {
            assert_eq!(streamed[key].get(), value.get(), "entry {key}");
        }
    }
}

#[tokio::test]
async fn eager_last_key_wins_but_stream_exposes_every_duplicate_entry() {
    let input =
        r#"{"ip":"512.768.1024.1280","\u0069p":"513.769.1025.1281","ip":"192.0.2.50","host":"512.768.1024.1280:9300"}"#;
    let (_fixture, receivers) = receivers_for(input).await;
    for receiver in receivers {
        let eager = receiver.get::<TypedDocument>().await.unwrap();
        assert_eq!(eager.0.len(), 2);
        assert_eq!(eager.0["ip"].get(), r#""192.0.2.50""#);
        assert_eq!(eager.0["host"].get(), r#""2.3.4.5""#);
        let entries = receiver
            .get_stream::<TypedDocument>()
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|(key, value)| (key.as_str(), value.get()))
                .collect::<Vec<_>>(),
            [
                ("ip", r#""2.3.4.5""#),
                ("ip", r#""3.4.5.6""#),
                ("ip", r#""192.0.2.50""#),
                ("host", r#""2.3.4.5""#),
            ]
        );
    }
}

#[tokio::test]
async fn eager_rejects_trailing_input_that_stream_consumer_leaves_unread() {
    let (_fixture, receivers) = receivers_for(r#"{"ip":"512.768.1024.1280"} trailing"#).await;
    for receiver in receivers {
        assert!(receiver.get::<TypedDocument>().await.is_err());
        let entries = receiver
            .get_stream::<TypedDocument>()
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .expect("stream consumes first object");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.get(), r#""2.3.4.5""#);
    }
}

#[tokio::test]
async fn truncated_or_invalid_json_reports_typed_errors_without_successful_partial_document() {
    for input in [
        r#"{"ip":"512.768.1024.1280""#,
        r#"{"ip":"512.768.1024.1280"#,
        r#"{"ip":tru}"#,
        r#"{"ip":"\q"}"#,
        r#"{"ip":"512.768.1024.1280","bad":[}"#,
    ] {
        let (_fixture, receivers) = receivers_for(input).await;
        for receiver in receivers {
            assert!(receiver.get::<TypedDocument>().await.is_err(), "eager accepted {input}");
            let items = receiver
                .get_stream::<TypedDocument>()
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await;
            assert!(items.iter().any(Result::is_err), "stream accepted {input}");
        }
    }
}

#[tokio::test]
async fn disabled_typed_consumers_keep_original_address_and_numeric_lexemes() {
    let (_fixture, receivers) =
        receivers_for_scrub(r#"{"ip":"512.768.1024.1280","number":1.2300e+08,"huge":1e9999}"#, false).await;
    for receiver in receivers {
        let eager = receiver.get::<TypedDocument>().await.unwrap();
        assert_eq!(eager.0["ip"].get(), r#""512.768.1024.1280""#);
        assert_eq!(eager.0["number"].get(), "1.2300e+08");
        assert_eq!(eager.0["huge"].get(), "1e9999");
        let entries = receiver
            .get_stream::<TypedDocument>()
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries[0].1.get(), r#""512.768.1024.1280""#);
        assert_eq!(entries[1].1.get(), "1.2300e+08");
        assert_eq!(entries[2].1.get(), "1e9999");
    }
}

#[tokio::test]
async fn typed_value_consumers_keep_stricter_depth_limits_within_the_normalization_budget() {
    let shallow = format!("{}0{}", "[".repeat(127), "]".repeat(127));
    let (_fixture, receivers) = receivers_for(&shallow).await;
    for receiver in receivers {
        let eager = receiver.get::<DepthDocument>().await.expect("shallow eager value");
        assert!(eager.0.is_array());
        let entries = receiver
            .get_stream::<DepthDocument>()
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries, [eager.0]);
    }

    let deep = format!("{}0{}", "[".repeat(128), "]".repeat(128));
    let (_fixture, receivers) = receivers_for(&deep).await;
    for receiver in receivers {
        assert!(receiver.get::<DepthDocument>().await.is_err());
        let entries = receiver
            .get_stream::<DepthDocument>()
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert!(entries.iter().any(Result::is_err));
    }
}
