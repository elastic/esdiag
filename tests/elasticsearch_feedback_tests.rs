// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde_json::{Value, json};

#[cfg(feature = "setup")]
#[tokio::test]
#[ignore = "installs assets into the Serverless project selected by ESDIAG_OUTPUT_* environment variables"]
async fn live_serverless_elasticsearch_setup() {
    let client = esdiag::client::Client::try_from(esdiag::data::Uri::try_from_output_env().unwrap()).unwrap();
    assert!(client.is_serverless().await.unwrap());
    // Serverless versions roll continuously. Probe deployment flavor and test
    // API acceptance instead of asserting a fixed version.number.
    esdiag::setup::assets(&client).await.unwrap();
    esdiag::setup::ensure_agent_builder_license(&client).await.unwrap();
}

fn asset(path: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/elasticsearch")
        .join(path);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
#[ignore = "requires installed assets in the Serverless project selected by ESDIAG_OUTPUT_*; creates and cleans isolated test streams"]
async fn live_serverless_rerouted_rejections_are_reported_and_retained() {
    use esdiag::{data::Uri, exporter::Exporter};
    let exporter = Exporter::try_from(Uri::try_from_output_env().unwrap()).unwrap();
    let namespace = format!("esdiag-feedback-{}", uuid::Uuid::new_v4());
    let mut templates = Vec::new();
    let mut streams = Vec::new();
    let result: eyre::Result<()> = async {
        for kind in ["indicator", "impact", "diagnosis"] {
            let stream = format!("health-{kind}-{namespace}");
            let name = format!("{namespace}-{kind}");
            let mut template = asset(&format!("index_templates/health-{kind}.json"));
            template["index_patterns"] = json!([stream]);
            template["priority"] = json!(501);
            template["template"]["mappings"]["properties"]["data_stream"]["properties"]["namespace"] =
                json!({"type":"constant_keyword","value":namespace});
            // Only isolated test templates carry the historical alias shape.
            template["composed_of"]
                .as_array_mut()
                .unwrap()
                .retain(|name| name != "esdiag@metadata");
            if kind != "indicator" {
                template["template"]["mappings"]["properties"]["diagnostic"] = json!({"properties":{
                    "platform":{"type":"alias","path":"diagnostic.orchestration"},
                    "orchestration":{"type":"keyword"}
                }});
            }
            templates.push(name.clone());
            streams.push(stream);
            let response = exporter
                .request("PUT", &format!("/_index_template/{name}"), Some(&template))
                .await?;
            eyre::ensure!(response.status_code().is_success(), "{}", response.text().await?);
        }
        let docs: Vec<_> = ["indicator", "impact", "diagnosis"]
            .iter()
            .map(|kind| {
                json!({
                    "@timestamp":chrono::Utc::now().to_rfc3339(),
                    "data_stream":{"type":"health","dataset":kind,"namespace":namespace},
                    "diagnostic":{"id":namespace,"platform":"self-managed"}
                })
            })
            .collect();
        let batch = exporter.send(streams[0].clone(), docs.clone()).await?;
        eyre::ensure!(
            batch.docs == 1 && batch.errors == 2,
            "expected only the indicator to index"
        );
        for stream in &streams[1..] {
            eyre::ensure!(
                batch.rejected_indices.get(stream) == Some(&1),
                "wrong destination attribution"
            );
            exporter
                .request("POST", &format!("/{stream}%3A%3Afailures/_refresh"), None)
                .await?;
            let response = exporter
                .request("GET", &format!("/{stream}%3A%3Afailures/_search"), None)
                .await?;
            eyre::ensure!(response.status_code().is_success(), "{}", response.text().await?);
            let failures: Value = response.json().await?;
            eyre::ensure!(failures["hits"]["total"]["value"] == 1, "{failures}");
            let source = &failures["hits"]["hits"][0]["_source"];
            eyre::ensure!(
                source["error"]["message"].as_str().unwrap().contains("alias"),
                "{source}"
            );
            eyre::ensure!(
                source["document"]["source"]["diagnostic"]["platform"] == "self-managed",
                "original document lost"
            );
            let response = exporter
                .request(
                    "PUT",
                    &format!("/_data_stream/{stream}/_options"),
                    Some(&json!({"failure_store":{"enabled":false}})),
                )
                .await?;
            eyre::ensure!(response.status_code().is_success(), "{}", response.text().await?);
        }
        let batch = exporter.send(streams[0].clone(), docs).await?;
        eyre::ensure!(
            batch.docs == 1 && batch.errors == 2,
            "expected direct mapping rejections"
        );
        for stream in &streams[1..] {
            eyre::ensure!(
                batch.rejected_indices.get(stream) == Some(&1),
                "wrong direct rejection destination"
            );
            eyre::ensure!(
                batch.rejection_reasons[stream]
                    .iter()
                    .any(|reason| reason.contains("alias")),
                "missing actual mapping reason: {:?}",
                batch.rejection_reasons
            );
        }
        Ok(())
    }
    .await;
    for path in streams
        .iter()
        .map(|name| format!("/_data_stream/{name}"))
        .chain(templates.iter().map(|name| format!("/_index_template/{name}")))
    {
        let response = exporter.request("DELETE", &path, None).await.unwrap();
        assert!(
            response.status_code().is_success() || response.status_code().as_u16() == 404,
            "cleanup {path}"
        );
    }
    result.unwrap();
}

#[test]
fn every_data_stream_inherits_the_failure_store() {
    let common = asset("component_templates/esdiag@settings.json");
    assert_eq!(
        common["template"]["data_stream_options"]["failure_store"]["enabled"],
        true
    );
    for entry in std::fs::read_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/elasticsearch/index_templates"
    ))
    .unwrap()
    {
        let entry = entry.unwrap();
        let template: Value = serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
        assert!(
            template["composed_of"]
                .as_array()
                .unwrap()
                .contains(&json!("esdiag@settings")),
            "{}",
            entry.path().display()
        );
        assert_ne!(
            template.pointer("/template/data_stream_options/failure_store/enabled"),
            Some(&json!(false))
        );
    }
}

/// Installs uniquely named test assets and deletes them after the assertions.
#[tokio::test]
#[ignore = "requires an isolated, unauthenticated Elasticsearch 9.4+ at ESDIAG_TEST_ELASTICSEARCH_URL"]
async fn settings_values_and_subsettings_index_and_failures_are_recoverable() {
    let base = std::env::var("ESDIAG_TEST_ELASTICSEARCH_URL").expect("test Elasticsearch URL");
    let client = reqwest::Client::new();
    for (template_path, documents, fields) in [
        (
            "index_templates/settings-cluster.json",
            vec![
                json!({"rest.incremental_bulk":"true","rest.incremental_bulk.request_timeout":"-1"}),
                json!({"rest":{"incremental_bulk":"true","incremental_bulk.request_timeout":"-1"}}),
            ],
            vec![
                ("rest.incremental_bulk", "true"),
                ("rest.incremental_bulk.request_timeout", "-1"),
            ],
        ),
        (
            "index_templates/settings-node.json",
            vec![
                json!({"node":{"settings":{"http.type":"netty4","http.type.default":"netty4","http.max_warning_header_size":"-1","transport.connect_timeout":"30s","transport.type":"netty4","transport.type.default":"netty4"}}}),
                json!({"node":{"settings":{"http":{"type":"netty4","type.default":"netty4","max_warning_header_size":"-1"},"transport":{"type":"netty4","type.default":"netty4","connect_timeout":"30s"}}}}),
            ],
            vec![
                ("node.settings.http.max_warning_header_size", "-1"),
                ("node.settings.transport.connect_timeout", "30s"),
                ("node.settings.http.type", "netty4"),
                ("node.settings.http.type.default", "netty4"),
                ("node.settings.transport.type", "netty4"),
                ("node.settings.transport.type.default", "netty4"),
            ],
        ),
    ] {
        let name = format!("esdiag-feedback-{}", uuid::Uuid::new_v4());
        let stream = format!("{name}-data");
        let mut components = Vec::new();
        let result: eyre::Result<()> = async {
        let mut template = asset(template_path);
        for original in template["composed_of"].as_array().unwrap() {
            let original = original.as_str().unwrap();
            let component = format!("{name}-{original}");
            components.push(component.clone());
            let response = client
                .put(format!("{base}/_component_template/{component}"))
                .json(&asset(&format!("component_templates/{original}.json")))
                .send()
                .await?;
            eyre::ensure!(response.status().is_success(), "{}", response.text().await?);
        }
        template["composed_of"] = json!(components);
        template["index_patterns"] = json!([stream]);
        let response = client
            .put(format!("{base}/_index_template/{name}"))
            .json(&template)
            .send()
            .await?;
        eyre::ensure!(response.status().is_success(), "{}", response.text().await?);

        for settings in documents {
            let mut doc = settings;
            doc["@timestamp"] = json!("2026-09-06T00:00:00Z");
            if template_path == "index_templates/settings-node.json" {
                doc["node"]["settings"]["suppression_check"] = json!({
                    "elapsed_time":"1s", "elapsed_time_string":"1s",
                    "buffer_size":"1kb", "buffer_memory":"1kb"
                });
            }
            let response = client
                .post(format!("{base}/{stream}/_doc?refresh=true"))
                .json(&doc)
                .send()
                .await?;
            eyre::ensure!(response.status().is_success(), "{}", response.text().await?);
            let body: Value = response.json().await?;
            eyre::ensure!(body.get("failure_store").is_none(), "settings were rejected: {body}");
        }
        let found: Value = client
            .post(format!("{base}/{stream}/_search"))
            .json(&json!({"query":{"bool":{"filter": fields.iter().map(|(field, value)| json!({"term":{*field:*value}})).collect::<Vec<_>>()}}}))
            .send()
            .await?
            .json()
            .await?;
        eyre::ensure!(
            found["hits"]["total"]["value"] == 2,
            "both setting values must be searchable: {found}"
        );
        if template_path == "index_templates/settings-node.json" {
            let mappings: Value = client.get(format!("{base}/{stream}/_mapping"))
                .send().await?.error_for_status()?.json().await?;
            for mapping in mappings.as_object().unwrap().values() {
                let properties = &mapping["mappings"]["properties"]["node"]["properties"]
                    ["settings"]["properties"]["suppression_check"]["properties"];
                for field in ["elapsed_time", "elapsed_time_string", "buffer_size", "buffer_memory"] {
                    eyre::ensure!(properties[field]["enabled"] == false,
                        "unmapped node settings must retain suppression: {properties}");
                }
            }
        }

        let rejected: Value = client
            .post(format!("{base}/{stream}/_doc?refresh=true"))
            .json(&json!({"@timestamp":"2026-09-06T00:00:00Z","diagnostic":{"id":{"invalid":"object"}}}))
            .send()
            .await?
            .json()
            .await?;
        eyre::ensure!(
            rejected["failure_store"] == "used",
            "rejected document must be retained: {rejected}"
        );
        let failures: Value = client
            .get(format!("{base}/{stream}::failures/_search"))
            .send()
            .await?
            .json()
            .await?;
        eyre::ensure!(
            failures["hits"]["hits"][0]["_source"]["document"]["source"]["diagnostic"]["id"]["invalid"] == "object",
            "original failed document must be recoverable: {failures}"
        );
        Ok(())
    }
    .await;
        for path in std::iter::once(format!("_data_stream/{stream}"))
            .chain(std::iter::once(format!("_index_template/{name}")))
            .chain(
                components
                    .iter()
                    .map(|component| format!("_component_template/{component}")),
            )
        {
            let response = client.delete(format!("{base}/{path}")).send().await.unwrap();
            assert!(
                response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND,
                "cleanup {path}"
            );
        }
        result.unwrap();
    }
}
