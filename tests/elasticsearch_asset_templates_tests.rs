use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn template_dataset(file_name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/elasticsearch/index_templates")
        .join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
    let template: Value =
        serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err));

    template["template"]["mappings"]["properties"]["data_stream"]["properties"]["dataset"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{} missing data_stream.dataset.value", path.display()))
        .to_string()
}

fn template_dataset_mapping(file_name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/elasticsearch/index_templates")
        .join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
    let template: Value =
        serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err));

    template["template"]["mappings"]["properties"]["data_stream"]["properties"]["dataset"].clone()
}

fn index_template(file_name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/elasticsearch/index_templates")
        .join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err))
}

fn component_template(file_name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/elasticsearch/component_templates")
        .join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err))
}

fn diagnostic_properties(template: &Value) -> &Value {
    &template["template"]["mappings"]["properties"]["diagnostic"]["properties"]
}

fn collect_rs_files(root: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).unwrap_or_else(|err| panic!("read dir {}: {}", root.display(), err)) {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn emitted_output_streams() -> BTreeSet<String> {
    let mut files = Vec::new();
    for root in ["src/processor", "src/exporter"] {
        collect_rs_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join(root), &mut files);
    }

    let stream_re =
        regex::Regex::new(r#""((?:metrics|settings|logs|health)-[A-Za-z0-9_.]+-esdiag)""#).expect("stream regex");
    let mut streams = BTreeSet::new();
    for path in files {
        let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
        for capture in stream_re.captures_iter(&content) {
            let stream = capture[1].to_string();
            // Metadata builders use this as a placeholder before processors set
            // their real destination stream; it is never emitted as output.
            if stream != "metrics-default-esdiag" {
                streams.insert(stream);
            }
        }
    }
    streams
}

fn index_template_patterns() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/elasticsearch/index_templates");
    let mut patterns = BTreeSet::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|err| panic!("read dir {}: {}", root.display(), err)) {
        let path = entry.expect("read directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
        let template: Value =
            serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err));
        let Some(index_patterns) = template["index_patterns"].as_array() else {
            panic!("{} missing index_patterns", path.display());
        };
        for pattern in index_patterns {
            patterns.insert(
                pattern
                    .as_str()
                    .unwrap_or_else(|| panic!("{} has non-string index pattern", path.display()))
                    .to_string(),
            );
        }
    }
    patterns
}

fn wildcard_matches(pattern: &str, stream: &str) -> bool {
    let Some((first, rest)) = pattern.split_once('*') else {
        return pattern == stream;
    };
    if !stream.starts_with(first) {
        return false;
    }

    let mut remaining = &stream[first.len()..];
    let mut parts = rest.split('*').peekable();
    while let Some(part) = parts.next() {
        if part.is_empty() {
            continue;
        }
        let Some(index) = remaining.find(part) else {
            return false;
        };
        remaining = &remaining[index + part.len()..];
        if parts.peek().is_none() && !pattern.ends_with('*') {
            return remaining.is_empty();
        }
    }
    pattern.ends_with('*') || remaining.is_empty()
}

fn stream_template_drift(streams: &BTreeSet<String>, patterns: &BTreeSet<String>) -> Vec<String> {
    let mut drift = Vec::new();
    for stream in streams {
        if !patterns.iter().any(|pattern| wildcard_matches(pattern, stream)) {
            drift.push(format!("stream {stream} has no matching index template"));
        }
    }
    for pattern in patterns {
        if !streams.iter().any(|stream| wildcard_matches(pattern, stream)) {
            drift.push(format!("index template pattern {pattern} matches no emitted stream"));
        }
    }
    drift
}

#[test]
fn node_derived_metrics_templates_use_matching_dataset_constants() {
    let templates = [
        ("metrics-node.transport.actions.json", "node.transport.actions"),
        ("metrics-node.http.clients.json", "node.http.clients"),
        (
            "metrics-node.discovery.cluster_applier.json",
            "node.discovery.cluster_applier",
        ),
        (
            "metrics-node.discovery.cluster_adaptive.json",
            "node.discovery.cluster_adaptive",
        ),
    ];

    for (file_name, expected_dataset) in templates {
        assert_eq!(template_dataset(file_name), expected_dataset);
    }
}

#[test]
fn logstash_templates_allow_concrete_logstash_datasets() {
    for file_name in ["settings-logstash.json", "metrics-logstash.json"] {
        let dataset = template_dataset_mapping(file_name);
        assert_eq!(dataset["type"].as_str(), Some("constant_keyword"));
        assert!(
            dataset.get("value").is_none(),
            "{file_name} must not pin all logstash sub-streams to data_stream.dataset=logstash"
        );
    }
}

#[test]
fn metadata_templates_accept_both_writer_generations() {
    let templates = [
        diagnostic_properties(&component_template("esdiag@metadata.json")).clone(),
        diagnostic_properties(&component_template("esdiag@ls-metadata.json")).clone(),
        diagnostic_properties(&index_template("metrics-diagnostic.json")).clone(),
    ];
    for properties in templates {
        for (current, legacy) in [("application", "product"), ("platform", "orchestration")] {
            for (from, to) in [(current, legacy), (legacy, current)] {
                assert_eq!(properties[from]["type"], "keyword", "{from} must accept writes");
                assert_eq!(properties[from]["copy_to"], format!("diagnostic.{to}"));
            }
        }
    }
}

/// Every provenance name a shipped saved object references, paired with the
/// object's path under the saved-objects directory so a failure names the file
/// to fix.
fn saved_object_provenance_usage() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kibana/esdiag/objects");
    let mut files = Vec::new();
    collect_json_files(&root, &mut files);

    let field_re = regex::Regex::new(r"diagnostic\.(product|application|orchestration|platform)").expect("field regex");
    let mut usage = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
        let label = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for capture in field_re.captures_iter(&content) {
            usage.push((label.clone(), capture[1].to_string()));
        }
    }
    usage
}

fn collect_json_files(root: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).unwrap_or_else(|err| panic!("read dir {}: {}", root.display(), err)) {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_json_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
}

/// The index pattern each shipped data view resolves against.
fn saved_data_view_patterns() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/kibana/esdiag/objects/index-pattern");
    let mut patterns = Vec::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|err| panic!("read dir {}: {}", root.display(), err)) {
        let path = entry.expect("read directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
        let object: Value =
            serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err));
        let title = object["attributes"]["title"]
            .as_str()
            .unwrap_or_else(|| panic!("{} missing attributes.title", path.display()))
            .to_string();
        patterns.push((
            path.file_name().expect("file name").to_string_lossy().to_string(),
            title,
        ));
    }
    patterns
}

/// The dashboard layer used to be unverifiable, so ADR-0015 left it to review
/// discipline. The saved objects now live in the repository, so the convention is
/// checkable: a title that does not pin `-esdiag` also matches indices ESDiag does
/// not own.
#[test]
fn shipped_data_views_follow_the_stream_naming_contract() {
    let convention = regex::Regex::new(r"^(metrics|settings|logs|health)-[A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*-esdiag\*?$")
        .expect("data view convention regex");

    let offenders: Vec<String> = saved_data_view_patterns()
        .into_iter()
        .filter(|(_, title)| !convention.is_match(title))
        .map(|(file, title)| format!("{file} queries {title}"))
        .collect();

    assert!(
        offenders.is_empty(),
        "data view titles must name a `{{class}}-{{subtype}}-esdiag` stream:\n{}",
        offenders.join("\n")
    );
}

/// Keep legacy fields searchable while shipped saved objects still use them.
#[test]
fn shipped_saved_objects_only_query_provenance_names_the_templates_define() {
    let properties = diagnostic_properties(&component_template("esdiag@metadata.json")).clone();
    let usage = saved_object_provenance_usage();

    for (object, field) in &usage {
        assert!(
            properties.get(field).is_some(),
            "{object} queries diagnostic.{field}, which no template defines"
        );
    }

    let legacy_names = ["product", "orchestration"];
    let still_legacy: BTreeSet<&str> = usage
        .iter()
        .filter(|(_, field)| legacy_names.contains(&field.as_str()))
        .map(|(_, field)| field.as_str())
        .collect();
    for legacy in legacy_names {
        let field_installed = properties.get(legacy).is_some();
        assert!(
            !still_legacy.contains(legacy) || field_installed,
            "saved objects still query diagnostic.{legacy}, so its field must stay installed"
        );
    }
}

#[test]
fn emitted_output_streams_and_index_templates_stay_in_sync() {
    let streams = emitted_output_streams();
    let patterns = index_template_patterns();
    let convention = regex::Regex::new(r"^(metrics|settings|logs|health)-[A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*-esdiag$")
        .expect("stream convention regex");

    for stream in &streams {
        assert!(
            convention.is_match(stream),
            "{stream} does not follow the ESDiag stream naming contract"
        );
    }

    let drift = stream_template_drift(&streams, &patterns);
    assert!(
        drift.is_empty(),
        "processor/template data-stream drift:\n{}",
        drift.join("\n")
    );
}

#[test]
fn kibana_metrics_template_maps_dashboard_metadata_and_keeps_payload_dynamic() {
    let template = index_template("metrics-kibana.json");
    let mappings = &template["template"]["mappings"];
    let properties = &mappings["properties"];
    let diagnostic = component_template("esdiag@metadata.json");

    assert_eq!(mappings["dynamic"], true);
    assert_eq!(properties["@timestamp"]["type"], "date");
    assert_eq!(
        properties["data_stream"]["properties"]["dataset"]["type"],
        "constant_keyword"
    );
    assert_eq!(properties["data_stream"]["properties"]["type"]["value"], "metrics");
    assert_eq!(properties["node"]["properties"]["id"]["type"], "keyword");
    assert_eq!(properties["node"]["properties"]["name"]["type"], "keyword");
    assert_eq!(
        properties["node"]["properties"]["version"]["properties"]["number"]["type"],
        "version"
    );
    assert_eq!(diagnostic_properties(&diagnostic)["version"]["type"], "keyword");

    let explicitly_mapped: BTreeSet<&str> = properties
        .as_object()
        .expect("Kibana template properties")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(explicitly_mapped, BTreeSet::from(["@timestamp", "data_stream", "node"]));
    assert!(
        template["composed_of"]
            .as_array()
            .expect("composed_of")
            .iter()
            .any(|component| component == "esdiag@mappings"),
        "version-dependent Kibana payloads must use the shared dynamic mappings"
    );
}

#[test]
fn stream_template_drift_check_reports_injected_drift() {
    let streams = BTreeSet::from(["metrics-node-esdiag".to_string(), "metrics-missing-esdiag".to_string()]);
    let patterns = BTreeSet::from([
        "metrics-node-esdiag*".to_string(),
        "settings-orphan-esdiag*".to_string(),
    ]);

    let drift = stream_template_drift(&streams, &patterns);
    assert!(
        drift
            .iter()
            .any(|message| message == "stream metrics-missing-esdiag has no matching index template")
    );
    assert!(
        drift
            .iter()
            .any(|message| message == "index template pattern settings-orphan-esdiag* matches no emitted stream")
    );
}
