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

fn canonical_diagnostic_field(properties: &Value, field: &str) -> String {
    let mapping = &properties[field];
    if mapping["type"].as_str() == Some("alias") {
        mapping["path"].as_str().expect("alias path").to_string()
    } else {
        format!("diagnostic.{field}")
    }
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
fn metadata_templates_map_new_provenance_fields_and_transitional_aliases() {
    let templates = [
        diagnostic_properties(&component_template("esdiag@metadata.json")).clone(),
        diagnostic_properties(&component_template("esdiag@ls-metadata.json")).clone(),
        diagnostic_properties(&index_template("metrics-diagnostic.json")).clone(),
    ];

    for properties in templates {
        assert_eq!(properties["platform"]["type"].as_str(), Some("keyword"));
        assert_eq!(properties["application"]["type"].as_str(), Some("keyword"));
        assert_eq!(properties["product"]["type"].as_str(), Some("alias"));
        assert_eq!(properties["product"]["path"].as_str(), Some("diagnostic.application"));
        assert_eq!(properties["orchestration"]["type"].as_str(), Some("alias"));
        assert_eq!(
            properties["orchestration"]["path"].as_str(),
            Some("diagnostic.platform")
        );
    }
}

#[test]
fn product_application_alias_resolution_covers_old_and_new_mapping_shapes() {
    let new_properties = diagnostic_properties(&component_template("esdiag@metadata.json")).clone();
    assert_eq!(
        canonical_diagnostic_field(&new_properties, "application"),
        canonical_diagnostic_field(&new_properties, "product")
    );

    let old_properties = serde_json::json!({
        "product": { "type": "keyword" },
        "application": { "type": "alias", "path": "diagnostic.product" }
    });
    assert_eq!(
        canonical_diagnostic_field(&old_properties, "application"),
        canonical_diagnostic_field(&old_properties, "product")
    );
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
