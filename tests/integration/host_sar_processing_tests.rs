use super::elasticsearch_asset_templates_tests::{component_template, index_template, mapped_fields};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::{TempDir, tempdir};

const SAR_STREAM: &str = "metrics-host.sar-esdiag.ndjson";

/// The 9.3.3 API bundle plus synthetic `syscalls/` output: two nodes share
/// one host on different ports, and a third runs elsewhere.
fn sar_bundle() -> TempDir {
    let root = tempdir().expect("bundle directory");
    let archive = fs::File::open("tests/archives/elasticsearch-api-diagnostics-9.3.3.zip").expect("fixture archive");
    zip::ZipArchive::new(archive)
        .expect("read archive")
        .extract(root.path())
        .expect("extract archive");

    let nodes_path = root.path().join("nodes.json");
    let mut nodes: Value = serde_json::from_str(&fs::read_to_string(&nodes_path).expect("nodes.json")).expect("parse");
    let template = nodes["nodes"]
        .as_object()
        .and_then(|nodes| nodes.values().next())
        .cloned()
        .expect("a node");
    let node = |name: &str, ip: &str, port: u16| {
        let mut node = template.clone();
        node["name"] = json!(name);
        node["host"] = json!(ip);
        node["ip"] = json!(ip);
        node["transport_address"] = json!(format!("{ip}:{port}"));
        node["http"]["publish_address"] = json!(format!("{ip}:{}", port - 100));
        node
    };
    nodes["nodes"] = json!({
        "node-a": node("instance-0", "10.0.0.10", 9300),
        "node-b": node("instance-1", "10.0.0.10", 9301),
        "node-c": node("instance-2", "10.0.0.20", 9300),
    });
    fs::write(&nodes_path, nodes.to_string()).expect("write nodes.json");

    let syscalls = root.path().join("syscalls");
    fs::create_dir_all(&syscalls).expect("syscalls directory");
    fs::copy("tests/fixtures/sar/all-sections.txt", syscalls.join("sar.txt")).expect("sar.txt");
    fs::write(
        syscalls.join("ss.txt"),
        "Netid State  Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n\
         tcp   LISTEN 0      128    10.0.0.10:9300    0.0.0.0:*\n\
         tcp   LISTEN 0      128    10.0.0.10:9301    0.0.0.0:*\n\
         tcp   LISTEN 0      128    0.0.0.0:22         0.0.0.0:*\n\
         tcp   LISTEN 0      128    127.0.0.1:631      0.0.0.0:*\n",
    )
    .expect("ss.txt");
    root
}

fn process(bundle: &Path) -> TempDir {
    let home = tempdir().expect("temp HOME");
    let output = tempdir().expect("temp output");
    let process = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args([
            "process",
            bundle.to_str().expect("bundle path"),
            output.path().to_str().expect("output path"),
        ])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .expect("run esdiag process");
    assert!(
        process.status.success(),
        "process failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    output
}

fn read_docs(output: &Path, file_name: &str) -> Vec<Value> {
    fs::read_to_string(output.join(file_name))
        .unwrap_or_else(|err| panic!("read {file_name}: {err}"))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("document JSON"))
        .collect()
}

/// Dotted paths of a document's leaf values; SAR keys are already dotted.
fn document_fields(value: &Value, prefix: &str, fields: &mut BTreeSet<String>) {
    for (name, value) in value.as_object().expect("document object") {
        let path = format!("{prefix}{name}");
        if value.is_object() {
            document_fields(value, &format!("{path}."), fields);
        } else {
            fields.insert(path);
        }
    }
}

fn sar_events(output: &Path) -> Vec<Value> {
    read_docs(output, "metrics-diagnostic-esdiag.ndjson")
        .iter()
        .flat_map(|doc| doc["diagnostic"]["events"].as_array().cloned().unwrap_or_default())
        .filter(|event| event["source"] == "syscalls.sar")
        .collect()
}

#[test]
fn every_sar_field_fits_the_template_field_limit() {
    let bundle = sar_bundle();
    let output = process(bundle.path());
    let mut leaves = BTreeSet::new();
    for doc in read_docs(output.path(), SAR_STREAM) {
        document_fields(&doc, "", &mut leaves);
    }
    let mut mapped = HashMap::new();
    let template = index_template("metrics-host.sar.json");
    mapped_fields(&template["template"]["mappings"]["properties"], "", &mut mapped);
    for component in ["esdiag@host.json", "esdiag@metadata.json", "esdiag@mappings.json"] {
        mapped_fields(
            &component_template(component)["template"]["mappings"]["properties"],
            "",
            &mut mapped,
        );
    }
    let limit = template["template"]["settings"]["index.mapping.total_fields.limit"]
        .as_u64()
        .expect("field limit") as usize;

    // Each dotted leaf also counts its parent objects.
    let mut total = BTreeSet::new();
    for path in leaves.iter().chain(mapped.keys()) {
        let mut end = 0;
        for part in path.split('.') {
            end += part.len();
            total.insert(&path[..end]);
            end += 1;
        }
    }
    assert!(
        total.len() < limit,
        "{} fields exceed the {limit} field limit",
        total.len()
    );
}

#[test]
fn sar_bundle_exports_host_documents_correlated_with_colocated_nodes() {
    let bundle = sar_bundle();
    let output = process(bundle.path());
    let docs = read_docs(output.path(), SAR_STREAM);

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for doc in &docs {
        *counts
            .entry(doc["sar.metricset"].as_str().expect("sar.metricset").to_string())
            .or_default() += 1;
    }
    // Two samples of every section: 2 CPUs, 3 devices, 2 interfaces, and
    // 2 `INTR` rows plus a 2-CPU by 4-interrupt matrix.
    let expected: BTreeMap<String, usize> = [
        ("summary", 2),
        ("cpu", 4),
        ("io", 6),
        ("network", 4),
        ("filesystem", 2),
        ("interrupt", 20),
        ("tty", 4),
        ("fan", 4),
        ("temp", 4),
        ("in", 4),
        ("usb", 2),
        ("fc_host", 2),
    ]
    .into_iter()
    .map(|(metricset, count)| (metricset.to_string(), count))
    .collect();
    assert_eq!(counts, expected);

    let summary_times: Vec<i64> = docs
        .iter()
        .filter(|doc| doc["sar.metricset"] == "summary")
        .map(|doc| doc["@timestamp"].as_i64().expect("@timestamp"))
        .collect();
    assert_eq!(summary_times, [1_785_888_610_000, 1_785_889_210_000]);

    for doc in &docs {
        assert_eq!(doc["data_stream"]["dataset"], "host.sar");
        assert_eq!(doc["host"]["name"], "node-01.example.com");
        assert_eq!(doc["host"]["ip"], json!(["10.0.0.10"]));
        assert_eq!(doc["elasticsearch"]["node"]["match"], "listen_socket");
        assert_eq!(doc["elasticsearch"]["node"]["id"], json!(["node-a", "node-b"]));
        assert_eq!(
            doc["elasticsearch"]["node"]["name"],
            json!(["instance-0", "instance-1"])
        );
        assert_eq!(doc["sar.timezone"], "assumed_utc");
        assert!(doc.get("node").is_none(), "SAR documents carry no node.* enrichment");
        assert_ne!(doc["diagnostic"]["collection_date"], doc["@timestamp"]);
    }

    let host_streams: Vec<String> = fs::read_dir(output.path())
        .expect("output directory")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with("metrics-host"))
        .collect();
    assert_eq!(host_streams, [SAR_STREAM]);

    let events = sar_events(output.path());
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["severity"], "success");
}

/// Resolve a field the way Kibana sees it: SAR metrics are flat dotted keys,
/// while the host and metadata envelopes are nested objects.
fn field<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    doc.get(path)
        .or_else(|| path.split('.').try_fold(doc, |value, segment| value.get(segment)))
}

const HOSTS_OVERVIEWS_LINKS: &str = "494ccfe4-71e4-45fe-ac56-3969d6247dd1";

pub(crate) fn host_activity_dashboards() -> Vec<(&'static str, Value)> {
    ["Compute", "Storage", "Network", "System"]
        .into_iter()
        .map(|name| {
            let path = format!("assets/kibana/esdiag/objects/dashboard/Host Activity - {name}.json");
            let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {path}: {err}"));
            (name, serde_json::from_str(&content).expect("dashboard JSON"))
        })
        .collect()
}

fn lens_panels(dashboard: &Value) -> impl Iterator<Item = &Value> {
    dashboard["attributes"]["panelsJSON"]
        .as_array()
        .expect("panels")
        .iter()
        .filter(|panel| panel["type"] == "lens" || panel["type"] == "vis")
}

#[test]
fn host_activity_dashboards_only_chart_fields_their_metricset_emits() {
    let bundle = sar_bundle();
    let output = process(bundle.path());
    let docs = read_docs(output.path(), SAR_STREAM);
    let metricset_re = regex::Regex::new(r#"^sar\.metricset : "([a-z_]+)"( and .+)?$"#).expect("metricset regex");

    for (name, dashboard) in host_activity_dashboards() {
        check_dashboard_fields(name, &dashboard, &docs, &metricset_re);
    }
}

fn check_dashboard_fields(name: &str, dashboard: &Value, docs: &[Value], metricset_re: &regex::Regex) {
    let mut panels = lens_panels(dashboard).peekable();
    assert!(panels.peek().is_some(), "{name} has no charts");
    for panel in panels {
        let title = panel["embeddableConfig"]["title"].as_str().expect("panel title");
        let title = &format!("{name} / {title}");
        let state = &panel["embeddableConfig"]["attributes"]["state"];
        let query = state["query"]["query"].as_str().expect("panel query");
        let metricset = &metricset_re
            .captures(query)
            .unwrap_or_else(|| panic!("{title} must filter on sar.metricset, got {query:?}"))[1];
        let scoped: Vec<&Value> = docs.iter().filter(|doc| doc["sar.metricset"] == metricset).collect();
        assert!(!scoped.is_empty(), "{title}: no {metricset} documents");

        let layers = state["datasourceStates"]["formBased"]["layers"]
            .as_object()
            .expect("layers");
        for column in layers
            .values()
            .flat_map(|layer| layer["columns"].as_object().expect("columns").values())
        {
            let Some(source) = column["sourceField"].as_str() else {
                continue;
            };
            if source == "@timestamp" {
                continue;
            }
            assert!(
                scoped.iter().all(|doc| field(doc, source).is_some()),
                "{title} charts {source}, which {metricset} documents do not carry"
            );
        }
    }

    let controls = dashboard["attributes"]["controlGroupInput"]["panelsJSON"]
        .as_object()
        .expect("controls");
    for control in controls.values() {
        let source = control["explicitInput"]["fieldName"].as_str().expect("control field");
        assert!(
            docs.iter().any(|doc| field(doc, source).is_some()),
            "{name}: control {source} matches no SAR document"
        );
    }
}

#[test]
fn cpu_dashboard_grid_charts_each_cpu_once() {
    let (_, cpu) = host_activity_dashboards().swap_remove(0);
    let cpu_re = regex::Regex::new(r#" and sar\.cpu\.id : "(\d+)"$"#).expect("cpu regex");
    let mut ids: Vec<u32> = lens_panels(&cpu)
        .filter_map(|panel| {
            let query = panel["embeddableConfig"]["attributes"]["state"]["query"]["query"].as_str()?;
            cpu_re.captures(query).map(|caps| caps[1].parse().expect("cpu id"))
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..128).collect::<Vec<_>>());
}

#[test]
fn host_dashboards_open_with_the_overview_and_hosts_links() {
    let read = |path: &str| -> Value {
        serde_json::from_str(&fs::read_to_string(path).expect("saved object")).expect("saved object JSON")
    };
    let links = read("assets/kibana/esdiag/objects/links/Hosts Overviews.json");
    assert_eq!(links["id"], HOSTS_OVERVIEWS_LINKS);
    let mut targets: Vec<&str> = links["references"]
        .as_array()
        .expect("link references")
        .iter()
        .filter_map(|reference| reference["id"].as_str())
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [
            "host-activity-cpu",
            "host-activity-network",
            "host-activity-storage",
            "host-activity-system",
        ]
    );

    let overview_links = read("assets/kibana/esdiag/objects/links/Elasticsearch.json");
    let last = overview_links["attributes"]["links"]
        .as_array()
        .and_then(|links| links.last())
        .expect("overview links");
    assert_eq!(last["label"], "Hosts");
    let target = overview_links["references"]
        .as_array()
        .expect("overview link references")
        .iter()
        .find(|reference| reference["name"] == last["destinationRefName"])
        .expect("Hosts link reference");
    assert_eq!(target["id"], "hosts-overview");

    let overview_links_id = overview_links["id"].as_str().expect("overview links id");
    let hosts_overview = read("assets/kibana/esdiag/objects/dashboard/Hosts Overview.json");
    let dashboards = host_activity_dashboards();
    for (name, dashboard) in dashboards
        .iter()
        .map(|(name, dashboard)| (*name, dashboard))
        .chain([("Hosts Overview", &hosts_overview)])
    {
        let references = dashboard["references"].as_array().expect("references");
        let mut panels: Vec<(i64, &str)> = dashboard["attributes"]["panelsJSON"]
            .as_array()
            .expect("panels")
            .iter()
            .filter(|panel| panel["type"] == "links" && panel["gridData"]["y"] == 0)
            .map(|panel| {
                let name = format!("{}:savedObjectRef", panel["panelIndex"].as_str().expect("panel index"));
                let id = references
                    .iter()
                    .find(|reference| reference["name"] == name)
                    .and_then(|reference| reference["id"].as_str())
                    .expect("links panel reference");
                (panel["gridData"]["x"].as_i64().expect("panel x"), id)
            })
            .collect();
        panels.sort_unstable();
        let ids: Vec<&str> = panels.into_iter().map(|(_, id)| id).collect();
        assert_eq!(
            ids,
            [overview_links_id, HOSTS_OVERVIEWS_LINKS],
            "{name} opens with the Elasticsearch links, then the Hosts Overviews links"
        );
    }
}

#[test]
fn bundle_without_syscalls_reports_nothing_for_sar() {
    let bundle = sar_bundle();
    fs::remove_dir_all(bundle.path().join("syscalls")).expect("remove syscalls");
    let output = process(bundle.path());

    assert!(!output.path().join(SAR_STREAM).exists());
    assert!(sar_events(output.path()).is_empty());
}
