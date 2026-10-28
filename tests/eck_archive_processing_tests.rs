use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

const PLATFORM: &str = "elastic-cloud-kubernetes";

const ECK_STREAMS: [(&str, usize); 9] = [
    ("settings-eck.operator-esdiag", 1),
    ("settings-eck.resource-esdiag", 3),
    ("settings-eck.nodeset-esdiag", 2),
    ("logs-eck.operator-esdiag", 7),
    ("metrics-kubernetes.node-esdiag", 3),
    ("metrics-kubernetes.pod-esdiag", 6),
    ("metrics-kubernetes.container-esdiag", 6),
    ("metrics-kubernetes.volume-esdiag", 3),
    ("logs-kubernetes.event-esdiag", 3),
];

/// Values planted in the fixture outside the allowlist: env vars, ConfigMap
/// data, secrets, annotations, managedFields, unparsed log payloads, the
/// operator's own image list, operator-named ConfigMaps in a workload
/// namespace, and fields nested in resource status.
const SENTINELS: [&str; 18] = [
    "env-plaintext-sentinel",
    "DEMO_PASSWORD",
    "podtemplate-env-sentinel",
    "webhook-env-sentinel",
    "configmap-data-sentinel",
    "configmap-script-sentinel",
    "configmap-ca-sentinel",
    "secret-type-sentinel",
    "label-sentinel",
    "annotation-sentinel-value",
    "managedfields-sentinel",
    "last-applied-sentinel",
    "stack-trace-sentinel",
    "stack-log-sentinel",
    "images-sentinel",
    "decoy-operator-sentinel",
    "decoy-license-sentinel",
    "status-sentinel",
];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archives/eck-diagnostics-synthetic")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create fixture copy dir");
    for entry in fs::read_dir(from).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy fixture file");
        }
    }
}

/// The fixture with the Elasticsearch 9.3.3 archive extracted at its
/// `includedDiagnostics` path, so the child job runs as it would from a
/// real ECK bundle.
fn bundle_with_child(root: &Path) -> PathBuf {
    let bundle = root.join("eck");
    copy_dir(&fixture_dir(), &bundle);
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archives/elasticsearch-api-diagnostics-9.3.3.zip");
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).expect("open es archive")).expect("read es archive");
    zip.extract(bundle.join("demo/elasticsearch/quickstart"))
        .expect("extract es archive");
    bundle
}

/// The fixture without its child diagnostic. The directory exporter truncates
/// a stream file per job, so the parent's report is only observable when no
/// child job writes after it.
fn parent_only_bundle(root: &Path) -> PathBuf {
    let bundle = root.join("eck-parent");
    copy_dir(&fixture_dir(), &bundle);
    let manifest_path = bundle.join("manifest.json");
    let mut manifest: Value = serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["includedDiagnostics"] = Value::Array(Vec::new());
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    bundle
}

fn process(bundle: &Path, output: &Path, home: &Path) -> Value {
    fs::create_dir_all(output).expect("create output dir");
    let process = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            bundle.to_str().expect("bundle path"),
            output.to_str().expect("output path"),
            "--format",
            "json",
        ])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .output()
        .expect("run esdiag process");
    assert!(
        process.status.success(),
        "process failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    let result: Value = serde_json::from_slice(&process.stdout).expect("process result");
    assert_eq!(result["diagnostic"]["outcome"], "complete", "{result}");
    assert_eq!(result["diagnostic"]["documents_failed"], 0, "{result}");
    result
}

fn read_stream(output: &Path, stream: &str) -> Vec<Value> {
    let path = output.join(format!("{stream}.ndjson"));
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("parse {}: {e}", path.display())))
        .collect()
}

struct Run {
    _root: TempDir,
    output: PathBuf,
    parent_output: PathBuf,
    parent_id: String,
    streams: BTreeMap<&'static str, Vec<Value>>,
}

fn run() -> Run {
    let root = tempdir().expect("tempdir");
    let home = root.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let output = root.path().join("out");
    let result = process(&bundle_with_child(root.path()), &output, &home);
    let parent_id = result["diagnostic"]["id"].as_str().expect("parent id").to_string();
    let streams = ECK_STREAMS
        .iter()
        .map(|(stream, _)| (*stream, read_stream(&output, stream)))
        .collect();

    let parent_output = root.path().join("out-parent");
    process(&parent_only_bundle(root.path()), &parent_output, &home);

    Run {
        _root: root,
        output,
        parent_output,
        parent_id,
        streams,
    }
}

fn find<'a>(docs: &'a [Value], pointer: &str, value: &str) -> &'a Value {
    docs.iter()
        .find(|doc| doc.pointer(pointer).and_then(Value::as_str) == Some(value))
        .unwrap_or_else(|| panic!("no document with {pointer}={value}"))
}

#[test]
fn eck_bundle_emits_the_platform_streams_joined_to_the_child() {
    let run = run();

    for (stream, expected) in ECK_STREAMS {
        let docs = &run.streams[stream];
        assert_eq!(docs.len(), expected, "{stream} document count");
        let (class, dataset) = stream
            .strip_suffix("-esdiag")
            .and_then(|name| name.split_once('-'))
            .unwrap();
        for doc in docs {
            assert_eq!(doc["data_stream"]["type"], class, "{stream}");
            assert_eq!(doc["data_stream"]["dataset"], dataset, "{stream}");
            assert_eq!(doc["diagnostic"]["id"], run.parent_id.as_str(), "{stream}");
            assert_eq!(doc["diagnostic"]["platform"], PLATFORM, "{stream}");
        }
    }

    let child_reports = read_stream(&run.output, "metrics-diagnostic-esdiag");
    let child = find(&child_reports, "/diagnostic/application", "elasticsearch");
    assert_eq!(child["diagnostic"]["parent_id"], run.parent_id.as_str());
    assert_eq!(child["diagnostic"]["platform"], PLATFORM);

    let containers = &run.streams["metrics-kubernetes.container-esdiag"];
    let oom = find(containers, "/last_terminated/reason", "OOMKilled");
    assert_eq!(oom["last_terminated"]["exit_code"], 137);
    assert_eq!(oom["container"]["restart_count"], 3);

    let pods = &run.streams["metrics-kubernetes.pod-esdiag"];
    let pending = find(pods, "/status/phase", "Pending");
    assert!(pending["kubernetes"]["node"].is_null(), "a Pending pod is unscheduled");

    let nodesets = &run.streams["settings-eck.nodeset-esdiag"];
    assert_eq!(find(nodesets, "/nodeset/name", "data")["rollout_in_progress"], true);
    assert_eq!(find(nodesets, "/nodeset/name", "master")["rollout_in_progress"], false);

    let volumes = &run.streams["metrics-kubernetes.volume-esdiag"];
    let resizing = find(volumes, "/claim/name", "elasticsearch-data-quickstart-es-data-0");
    assert_eq!(resizing["resize_pending"], true);

    let license = &run.streams["settings-eck.operator-esdiag"][0]["eck"]["license"];
    assert!(license["enterprise_resource_units"].is_number(), "{license}");
    assert!(license["total_managed_memory_bytes"].is_number(), "{license}");

    let parent_reports = read_stream(&run.parent_output, "metrics-diagnostic-esdiag");
    let [parent] = parent_reports.as_slice() else {
        panic!("expected one parent report, got {}", parent_reports.len());
    };
    assert!(parent["diagnostic"]["application"].is_null(), "{parent}");
    assert_eq!(
        parent["diagnostic"]["processor"]["stats"]["logs-eck.operator-esdiag"]["skipped_records"],
        3
    );
}

#[test]
fn eck_output_contains_only_allowlisted_fields() {
    let fixture: String = walk(&fixture_dir())
        .iter()
        .map(|path| fs::read_to_string(path).unwrap_or_default())
        .collect();
    for sentinel in SENTINELS {
        assert!(fixture.contains(sentinel), "fixture no longer plants {sentinel}");
    }

    let run = run();
    for output in [&run.output, &run.parent_output] {
        for path in walk(output) {
            let contents = fs::read_to_string(&path).unwrap();
            for sentinel in SENTINELS.iter().chain(&["sentinel"]) {
                assert!(!contents.contains(sentinel), "{} leaked {sentinel}", path.display());
            }
        }
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
