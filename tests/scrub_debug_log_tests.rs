//! Debug-log assertions for scrub normalization (OpenSpec task 4.5).
//! Runs in its own test binary so tracing capture is not shared with lib unit tests.

use esdiag::data::Uri;
use esdiag::processor::Nodes;
use esdiag::receiver::{ReceiveRaw, Receiver};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tracing_subscriber::{Layer, layer::SubscriberExt};
use zip::{write::SimpleFileOptions, ZipWriter};

struct CaptureLayer {
    logs: Arc<Mutex<Vec<String>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);
        if !message.is_empty() {
            self.logs.lock().expect("log capture lock").push(message);
        }
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}").trim_matches('"'));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        }
    }
}

fn log_capture() -> Arc<Mutex<Vec<String>>> {
    static CAPTURE: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let logs = Arc::new(Mutex::new(Vec::new()));
            let layer = CaptureLayer {
                logs: logs.clone(),
            };
            let subscriber = tracing_subscriber::registry::Registry::default().with(layer);
            let _ = tracing::subscriber::set_global_default(subscriber);
            logs
        })
        .clone()
}

fn write_scrubbed_nodes_zip(path: &PathBuf, nodes_json: &str) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    let prefix = "api-diagnostics-scrub-log-test";

    zip.start_file(format!("{prefix}/diagnostic_manifest.json"), options)
        .expect("manifest");
    zip.write_all(
        br#"{"mode":"full","product":"elasticsearch","type":"elasticsearch_diagnostic","runner":"cli","version":"9.1.3","timestamp":"2025-09-18T00:18:07.432Z"}"#,
    )
    .expect("manifest body");

    zip.start_file(format!("{prefix}/version.json"), options)
        .expect("version");
    zip.write_all(
        br#"{"name":"esdiag-node","cluster_name":"esdiag-cluster","cluster_uuid":"aukedefkRcGa0BT16uuuNQ","version":{"number":"9.1.3","build_flavor":"default","build_type":"docker","build_hash":"abc","build_date":"2025-01-01T00:00:00.000000000Z","build_snapshot":false,"lucene_version":"10.2.2","minimum_wire_compatibility_version":"8.19.0","minimum_index_compatibility_version":"8.0.0"},"tagline":"You Know, for Search"}"#,
    )
    .expect("version body");

    zip.start_file(format!("{prefix}/nodes.json"), options)
        .expect("nodes");
    zip.write_all(nodes_json.as_bytes()).expect("nodes body");

    zip.finish().expect("finish zip");
}

#[tokio::test]
async fn scrubbed_read_emits_mode_and_file_unscrubbed_debug_logs() {
    let logs = log_capture();
    logs.lock().expect("log capture lock").clear();

    const MALFORMED_IP: &str = "512.768.1024.1280";
    const NODE_ID: &str = "aaaabbbbccccddddee0";
    let nodes_json = format!(
        r#"{{
  "_nodes": {{"total": 1, "successful": 1, "failed": 0}},
  "nodes": {{
    "{NODE_ID}": {{
      "name": "{NODE_ID}",
      "transport_address": "{MALFORMED_IP}:19840",
      "host": "{MALFORMED_IP}",
      "ip": "{MALFORMED_IP}",
      "version": "9.1.3",
      "build_flavor": "default",
      "build_hash": "abc",
      "build_type": "docker",
      "roles": ["data_hot", "master"],
      "os": {{"refresh_interval_in_millis": 1000, "available_processors": 8, "allocated_processors": 8}},
      "jvm": {{}},
      "process": {{}},
      "thread_pool": {{}}
    }}
  }}
}}"#
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let zip_path = dir.path().join("synthetic-malformed-ips-test.zip");
    write_scrubbed_nodes_zip(&zip_path, &nodes_json);

    let receiver = Receiver::try_from_with_scrub(Uri::File(zip_path), Some(true), None)
        .expect("receiver");
    let archive = match receiver {
        Receiver::ArchiveFile(r) => r,
        _ => panic!("expected archive file receiver"),
    };
    archive
        .set_source_product("elasticsearch")
        .expect("source product");

    let _raw = archive.get_raw::<Nodes>().await.expect("get raw nodes");

    let captured = logs.lock().expect("log capture lock");
    assert!(
        captured
            .iter()
            .any(|line| line.contains("Scrub normalization enabled") && line.contains("explicit true")),
        "expected mode-context log, got: {captured:?}"
    );
    assert!(
        captured.iter().any(|line| line.contains("scrubbed mode")),
        "expected scrubbed-mode file read log, got: {captured:?}"
    );
    assert!(
        captured
            .iter()
            .any(|line| line.contains("Unscrubbed") && line.contains("nodes.json")),
        "expected per-file unscrubbed count log, got: {captured:?}"
    );
}
