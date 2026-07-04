use serde_json::Value;
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
