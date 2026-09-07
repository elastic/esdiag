// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::*;
use std::collections::BTreeSet;

#[test]
fn kibana_setup_targets_default_and_named_spaces_without_losing_assets() {
    let original = kibana_bundle(&EmbeddedAssets::new().unwrap())
        .unwrap()
        .read_all()
        .unwrap();
    for target in [None, Some("support"), Some("esdiag")] {
        let mut bundle = original.clone();
        target_kibana_bundle(&mut bundle, target).unwrap();
        let destination = target.unwrap_or("default");
        assert_eq!(bundle.by_space.len(), 1);
        let assets = &bundle.by_space[destination];
        let source = &original.by_space["esdiag"];
        assert_eq!(assets.saved_objects.len(), source.saved_objects.len());
        assert_eq!(assets.workflows.len(), source.workflows.len());
        assert_eq!(assets.agents.len(), source.agents.len());
        assert_eq!(assets.tools.len(), source.tools.len());
        assert_eq!(assets.skills.len(), source.skills.len());
        if target.is_none() {
            assert!(bundle.spaces.is_empty());
            assert_eq!(
                default_agent_path(destination),
                "api/agent_builder/agents/elastic-ai-agent"
            );
        } else {
            assert_eq!(bundle.spaces[0]["id"], destination);
            assert_eq!(
                default_agent_path(destination),
                format!("s/{destination}/api/agent_builder/agents/elastic-ai-agent")
            );
        }
        if destination != "esdiag" {
            for value in assets
                .saved_objects
                .iter()
                .chain(&assets.workflows)
                .chain(&assets.agents)
                .chain(&assets.tools)
                .chain(&assets.skills)
            {
                assert!(!value.to_string().contains("/s/esdiag/"));
            }
            let skill_text = serde_json::to_string(&assets.skills).unwrap();
            let expected = target
                .map(|space| format!("/s/{space}/app/dashboards"))
                .unwrap_or_else(|| "/app/dashboards".to_string());
            assert!(skill_text.contains(&expected));
        } else {
            assert_eq!(bundle, original);
        }
    }
}

#[test]
fn default_agent_update_preserves_configuration_without_echoing_read_only_fields() {
    let original = serde_json::json!({
        "id": "elastic-ai-agent", "readonly": true,
        "access_control": {"entries": [], "access_mode": "public"},
        "configuration": {"instructions": "existing", "tools": [{"tool_ids": ["platform.core.*"]}], "skill_ids": ["existing-skill"]}
    });
    let skills = vec!["agentic-diagnostic-assistant".into()];
    let update = default_agent_skill_update(original.clone(), &skills).unwrap();
    assert_eq!(update.as_object().unwrap().len(), 1);
    assert_eq!(
        update["configuration"]["instructions"],
        original["configuration"]["instructions"]
    );
    assert_eq!(update["configuration"]["tools"], original["configuration"]["tools"]);
    assert_eq!(
        update["configuration"]["skill_ids"],
        serde_json::json!(["existing-skill", "agentic-diagnostic-assistant"])
    );
    assert_eq!(default_agent_skill_update(update.clone(), &skills).unwrap(), update);
}

fn visit_json(value: &Value, visitor: &mut impl FnMut(&Value)) {
    visitor(value);
    match value {
        Value::Object(fields) => fields.values().for_each(|value| visit_json(value, visitor)),
        Value::Array(values) => values.iter().for_each(|value| visit_json(value, visitor)),
        Value::String(text) => {
            if let Ok(decoded) = serde_json::from_str::<Value>(text) {
                visit_json(&decoded, visitor);
            }
        }
        _ => {}
    }
}

#[test]
fn dashboard_links_have_one_reference_and_no_obsolete_saved_object_id() {
    let bundle = kibana_bundle(&EmbeddedAssets::new().unwrap())
        .unwrap()
        .read_all()
        .unwrap();
    let objects = &bundle.by_space["esdiag"].saved_objects;
    let mut links_count = 0;
    for dashboard in objects.iter().filter(|object| object["type"] == "dashboard") {
        let references = dashboard["references"].as_array().unwrap();
        let unique: std::collections::BTreeSet<_> = references.iter().map(Value::to_string).collect();
        assert_eq!(
            unique.len(),
            references.len(),
            "{} has duplicate references",
            dashboard["id"]
        );
        visit_json(&dashboard["attributes"]["panelsJSON"], &mut |panel| {
            if panel["type"] != "links" || panel.get("panelIndex").is_none() {
                return;
            }
            links_count += 1;
            assert!(
                panel["embeddableConfig"].get("savedObjectId").is_none(),
                "{}",
                dashboard["id"]
            );
            if panel["embeddableConfig"]["links"]
                .as_array()
                .is_some_and(|links| !links.is_empty())
                || panel["embeddableConfig"]["attributes"]["links"]
                    .as_array()
                    .is_some_and(|links| !links.is_empty())
            {
                return;
            }
            let name = format!(
                "{}:{}",
                panel["panelIndex"].as_str().unwrap(),
                panel["panelRefName"].as_str().unwrap_or("savedObjectRef")
            );
            let matches: Vec<_> = references
                .iter()
                .filter(|reference| reference["type"] == "links" && reference["name"] == name)
                .collect();
            assert_eq!(matches.len(), 1, "{}: {name}", dashboard["id"]);
            assert!(
                objects
                    .iter()
                    .any(|object| object["type"] == "links" && object["id"] == matches[0]["id"])
            );
        });
    }
    assert!(links_count > 20);
}

#[test]
fn kibana_queries_do_not_use_serverless_unsupported_aggregations() {
    let bundle = kibana_bundle(&EmbeddedAssets::new().unwrap())
        .unwrap()
        .read_all()
        .unwrap();
    for object in &bundle.by_space["esdiag"].saved_objects {
        visit_json(object, &mut |value| {
            if let Some(spec) = value.get("spec").and_then(Value::as_str) {
                serde_json::from_str::<Value>(spec).expect("Vega specs must be inspectable JSON");
            }
            assert!(
                value.get("scripted_metric").is_none(),
                "{} uses scripted_metric",
                object["id"]
            );
            assert!(
                !matches!(value.get("type").and_then(Value::as_str), Some("timelion")),
                "{} needs a Serverless query review",
                object["id"]
            );
        });
    }
}

async fn live_json(client: &Client, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
    let body = body.map(|body| serde_json::to_vec(&body)).transpose()?;
    let mut headers = default_headers();
    headers.insert("X-Elastic-Internal-Origin".into(), "Kibana".into());
    let response = client.request(method, &headers, path, body.as_deref()).await?;
    let status = response.status();
    let value: Value = response.json().await?;
    eyre::ensure!(status.is_success(), "{path}: {status}: {value}");
    Ok(value)
}

#[tokio::test]
#[ignore = "reads diagnostic streams in the Serverless project selected by ESDIAG_OUTPUT_* environment variables"]
async fn live_serverless_dashboard_searches() -> Result<()> {
    let es = Client::try_from(crate::data::Uri::try_from_output_env()?)?;
    assert!(es.is_serverless().await?);
    let bundle = kibana_bundle(&EmbeddedAssets::new()?)?.read_all()?;
    let mut searches = Vec::new();
    for object in &bundle.by_space["esdiag"].saved_objects {
        visit_json(object, &mut |value| {
            if let Some(url) = value.get("url")
                && let (Some(index), Some(body)) = (url.get("index").and_then(Value::as_str), url.get("body"))
            {
                searches.push((index.to_string(), body.clone()));
            }
        });
    }
    assert!(!searches.is_empty());
    for (index, body) in &searches {
        let response = live_json(
            &es,
            Method::POST,
            &format!("/{index}/_search?ignore_unavailable=true&allow_no_indices=true"),
            Some(body.clone()),
        )
        .await?;
        assert_eq!(response["_shards"]["failed"], 0, "{index}: {response}");
    }
    eprintln!("Executed {} embedded dashboard searches", searches.len());
    Ok(())
}

/// Read and simulate the assets installed by `esdiag setup`; never creates diagnostic data.
#[tokio::test]
#[ignore = "requires explicit Serverless saved hosts or ESDIAG_OUTPUT_* and ESDIAG_KIBANA_URL, and installed assets"]
async fn live_serverless_asset_audit() -> Result<()> {
    use crate::data::Uri;
    let es = Client::try_from(match std::env::var("ESDIAG_SERVERLESS_TEST_HOST") {
        Ok(host) => Uri::try_from(host)?,
        Err(_) => Uri::try_from_output_env()?,
    })?;
    let kb = Client::try_from(match std::env::var("ESDIAG_SERVERLESS_TEST_KIBANA_HOST") {
        Ok(host) => Uri::try_from(host)?,
        Err(_) => Uri::try_from_kibana_env()?,
    })?;
    assert!(es.is_serverless().await?);
    assert!(kb.is_serverless().await?);
    ensure_agent_builder_license(&es).await?;
    let store = EmbeddedAssets::new()?;
    let assets = parse_assets_yml(Application::Elasticsearch, &store)?;
    let mut installed = 0;
    let mut simulated = 0;
    for asset in assets.iter().filter(|asset| !asset.requires_security) {
        for (path, _) in store.get_dir_files(&PathBuf::from(format!("elasticsearch/{}", asset.name))) {
            let stem = path.file_stem().unwrap().to_str().unwrap();
            let name = format!("{stem}{}", asset.suffix.as_deref().unwrap_or(""));
            let path = format!("/{}/{name}", asset.endpoint);
            live_json(&es, Method::GET, &path, None).await?;
            installed += 1;
            if asset.name == "index_templates" {
                let template =
                    live_json(&es, Method::POST, &format!("/_index_template/_simulate/{name}"), None).await?;
                let mut keys = BTreeSet::new();
                settings_keys(&template["template"]["settings"], "", &mut keys);
                assert!(!keys.contains("index.lifecycle.name"));
                assert!(!keys.contains("index.lifecycle.prefer_ilm"));
                simulated += 1;
            }
        }
    }
    eprintln!("Elasticsearch: read {installed} assets, simulated {simulated} composed index templates");
    let simulation = live_json(
        &es,
        Method::POST,
        "/_ingest/pipeline/esdiag/_simulate",
        Some(serde_json::json!({"docs":[{
            "_index":"metrics-diagnostic-esdiag", "_source": {
                "@timestamp":"2026-09-04T00:00:00Z", "diagnostic":{"license":{"issued_to":"serverless-audit"}},
                "data_stream":{"type":"metrics","dataset":"diagnostic","namespace":"esdiag"}
            }
        }]})),
    )
    .await?;
    assert!(simulation["docs"][0].get("error").is_none(), "{simulation}");
    assert!(simulation["docs"][0]["doc"]["_source"]["event"]["ingested"].is_string());
    assert_eq!(
        simulation["docs"][0]["doc"]["_source"]["diagnostic"]["account"],
        "serverless-audit"
    );
    let mut bundle = kibana_bundle(&store)?.read_all()?;
    target_kibana_bundle(&mut bundle, crate::env::get_kibana_space().as_deref())?;
    let mut query_count = 0;
    for (space_id, space) in &bundle.by_space {
        let prefix = if space_id == "default" {
            String::new()
        } else {
            format!("/s/{space_id}")
        };
        let types: BTreeSet<_> = space
            .saved_objects
            .iter()
            .map(|object| object["type"].as_str().unwrap())
            .collect();
        let mut imported = Vec::new();
        for kind in types {
            let mut page = 1;
            loop {
                let found = live_json(
                    &kb,
                    Method::GET,
                    &format!("{prefix}/api/saved_objects/_find?type={kind}&per_page=100&page={page}"),
                    None,
                )
                .await?;
                let objects = found["saved_objects"].as_array().unwrap();
                imported.extend(objects.iter().cloned());
                if page * 100 >= found["total"].as_u64().unwrap() {
                    break;
                }
                page += 1;
            }
        }
        for expected in &space.saved_objects {
            // Kibana remaps globally occupied IDs on import into another space.
            let actual = imported
                .iter()
                .find(|object| {
                    object["type"] == expected["type"]
                        && (object["id"] == expected["id"] || object["originId"] == expected["id"])
                })
                .unwrap_or_else(|| panic!("Missing imported {}/{}", expected["type"], expected["id"]));
            for reference in actual["references"].as_array().unwrap() {
                assert!(
                    imported
                        .iter()
                        .any(|object| object["type"] == reference["type"] && object["id"] == reference["id"]),
                    "{} has missing reference {reference}",
                    actual["id"]
                );
            }
            if expected["type"] == "dashboard" {
                let id = actual["id"].as_str().unwrap();
                let transformed = live_json(&kb, Method::GET, &format!("{prefix}/api/dashboards/{id}"), None).await?;
                for warning in transformed["warnings"].as_array().into_iter().flatten() {
                    assert_ne!(warning["panel_type"], "links", "{id}: {warning}");
                }
                let mut expected_links = 0;
                visit_json(&expected["attributes"]["panelsJSON"], &mut |panel| {
                    if panel["type"] == "links" && panel.get("panelIndex").is_some() {
                        expected_links += 1;
                    }
                });
                let mut actual_links = 0;
                visit_json(&transformed["data"], &mut |panel| {
                    if panel["type"] == "links" && panel.get("config").is_some() {
                        actual_links += 1;
                    }
                });
                assert_eq!(actual_links, expected_links, "{id}: navigation panels were lost");
            }
        }
        for (kind, values) in [
            ("workflows/workflow", &space.workflows),
            ("agent_builder/tools", &space.tools),
            ("agent_builder/skills", &space.skills),
        ] {
            for value in values {
                let id = value["id"].as_str().unwrap();
                live_json(&kb, Method::GET, &format!("{prefix}/api/{kind}/{id}"), None).await?;
            }
        }
        let agent = live_json(
            &kb,
            Method::GET,
            &format!("{prefix}/api/agent_builder/agents/elastic-ai-agent"),
            None,
        )
        .await?;
        for skill in &space.skills {
            assert!(
                agent["configuration"]["skill_ids"]
                    .as_array()
                    .unwrap()
                    .contains(&skill["id"])
            );
        }
        let mut searches = Vec::new();
        for object in &space.saved_objects {
            visit_json(object, &mut |value| {
                if let Some(url) = value.get("url")
                    && let (Some(index), Some(body)) = (url.get("index").and_then(Value::as_str), url.get("body"))
                {
                    searches.push((index.to_string(), body.clone()));
                }
            });
        }
        for (index, body) in searches {
            live_json(
                &es,
                Method::POST,
                &format!("/{index}/_search?ignore_unavailable=true&allow_no_indices=true"),
                Some(body),
            )
            .await?;
            query_count += 1;
        }
        eprintln!(
            "Kibana: read {} saved objects, {} workflow(s), {} tool(s), {} skill(s); verified default-agent attachment",
            space.saved_objects.len(),
            space.workflows.len(),
            space.tools.len(),
            space.skills.len()
        );
    }
    eprintln!("Executed {query_count} embedded Vega searches and simulated the ingest pipeline");
    Ok(())
}

fn settings_keys(value: &Value, prefix: &str, keys: &mut BTreeSet<String>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            settings_keys(value, &path, keys);
        }
    } else {
        keys.insert(prefix.to_string());
    }
}

#[test]
fn diagnostic_reports_use_serverless_compatible_retention() {
    let source = Assets::get("elasticsearch/index_templates/metrics-diagnostic.json").unwrap();
    let original: Value = serde_json::from_slice(&source.data).unwrap();
    let store = EmbeddedAssets::new().unwrap();
    let assets = parse_assets_yml(Application::Elasticsearch, &store).unwrap();
    let asset = assets.iter().find(|asset| asset.name == "index_templates").unwrap();
    let adapted: Value = serde_json::from_slice(&serverless_asset_contents(asset, &source.data).unwrap()).unwrap();
    assert_eq!(
        original["template"]["lifecycle"],
        serde_json::json!({"enabled": false, "data_retention": null})
    );
    assert_eq!(
        adapted["template"]["lifecycle"],
        serde_json::json!({"enabled": true, "data_retention": "3650d"})
    );
}

#[test]
fn serverless_templates_only_use_audited_settings() {
    // Reviewed against https://www.elastic.co/docs/reference/elasticsearch/index-settings/serverless.
    // New settings should receive a compatibility review before extending this set.
    let allowed = BTreeSet::from(
        [
            "index.codec",
            "index.mapping.source.mode",
            "index.mapping.ignore_malformed",
            "index.mapping.total_fields.limit",
            "index.mapping.total_fields.ignore_dynamic_beyond_limit",
            "index.query.default_field",
        ]
        .map(str::to_string),
    );
    let store = EmbeddedAssets::new().unwrap();
    let assets = parse_assets_yml(Application::Elasticsearch, &store).unwrap();
    let mut count = 0;
    for asset in assets
        .iter()
        .filter(|asset| matches!(asset.name.as_str(), "component_templates" | "index_templates"))
    {
        for (path, contents) in store.get_dir_files(&PathBuf::from(format!("elasticsearch/{}", asset.name))) {
            let original: Value = serde_json::from_slice(&contents).unwrap();
            let adapted: Value = serde_json::from_slice(&serverless_asset_contents(asset, &contents).unwrap()).unwrap();
            let mut keys = BTreeSet::new();
            if let Some(settings) = adapted.pointer("/template/settings") {
                settings_keys(settings, "", &mut keys);
            }
            assert!(
                keys.is_subset(&allowed),
                "{}: unaudited settings {:?}",
                path.display(),
                keys.difference(&allowed).collect::<Vec<_>>()
            );
            assert_eq!(
                original.pointer("/template/mappings"),
                adapted.pointer("/template/mappings"),
                "{} changed diagnostic fields",
                path.display()
            );
            if original.pointer("/template/lifecycle/enabled") != Some(&Value::Bool(false)) {
                assert_eq!(
                    original.pointer("/template/lifecycle"),
                    adapted.pointer("/template/lifecycle")
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 33);
}

#[test]
fn serverless_ilm_filter_handles_nested_and_flat_settings_without_removing_retention() {
    for settings in [
        serde_json::json!({"index":{"lifecycle":{"name":"metrics","prefer_ilm":false,"origination_date":123},"codec":"best_compression"}}),
        serde_json::json!({"index.lifecycle.name":"metrics","index.lifecycle.prefer_ilm":false,"index.lifecycle.origination_date":123,"index.codec":"best_compression"}),
        serde_json::json!({"index":{"lifecycle.name":"metrics","lifecycle.prefer_ilm":false,"lifecycle.origination_date":123,"codec":"best_compression"}}),
    ] {
        let mut settings = settings;
        remove_serverless_ilm_settings(&mut settings, "");
        let mut keys = BTreeSet::new();
        settings_keys(&settings, "", &mut keys);
        assert_eq!(
            keys,
            BTreeSet::from(["index.lifecycle.origination_date".into(), "index.codec".into()])
        );
    }
    let source = Assets::get("elasticsearch/component_templates/esdiag@settings.json").unwrap();
    let source: Value = serde_json::from_slice(&source.data).unwrap();
    assert_eq!(
        source.pointer("/template/settings/index/lifecycle/prefer_ilm"),
        Some(&Value::Bool(false))
    );
    assert_eq!(source.pointer("/template/lifecycle/data_retention").unwrap(), "30d");
}

#[test]
fn serverless_kibana_only_removes_stateful_space_controls() {
    let store = EmbeddedAssets::new().unwrap();
    let original = kibana_bundle(&store).unwrap().read_all().unwrap();
    let mut adapted = original.clone();
    adapt_serverless_kibana_bundle(&mut adapted);
    assert_eq!(original.spaces[0]["solution"], "oblt");
    assert!(original.spaces[0]["disabledFeatures"].as_array().unwrap().len() > 0);
    for (before, after) in original.spaces.iter().zip(&adapted.spaces) {
        let mut expected = before.clone();
        expected.as_object_mut().unwrap().remove("solution");
        expected.as_object_mut().unwrap().remove("disabledFeatures");
        assert_eq!(&expected, after);
    }
    let before = &original.by_space["esdiag"];
    let after = &adapted.by_space["esdiag"];
    assert_eq!(before.saved_objects, after.saved_objects);
    assert_eq!(before.workflows, after.workflows);
    assert_eq!(before.tools, after.tools);
    assert_eq!(before.skills, after.skills);
    assert_eq!(before.agents, after.agents);
}
