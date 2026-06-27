use serde_json::Value;
use std::fs;
use std::path::Path;

fn template_dataset(file_name: &str) -> String {
    let path = Path::new("assets/elasticsearch/index_templates").join(file_name);
    let content = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {}", path.display(), err));
    let template: Value =
        serde_json::from_str(&content).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err));

    template["template"]["mappings"]["properties"]["data_stream"]["properties"]["dataset"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{} missing data_stream.dataset.value", path.display()))
        .to_string()
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
