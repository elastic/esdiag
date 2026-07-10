// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    client::Client,
    data::Product,
    embeds::{Assets, KIBANA_ASSETS_BUNDLE},
};
//use bytes::Bytes;
use eyre::{Result, WrapErr, eyre};
use kibana_sync::{
    KibanaBundle,
    sync::{SyncBundle, SyncOptions, SyncSummary, push_sync},
};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

// Subdirectory for templates and configs files
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";
const KIBANA_ASSETS_DIR: &str = "kibana";

struct EmbeddedAssets;

impl EmbeddedAssets {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    fn get_file(&self, path: &Path) -> Option<std::borrow::Cow<'static, [u8]>> {
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with(KIBANA_ASSETS_DIR) {
                return get_kibana_bundle_file(path_str);
            }
            Assets::get(path_str).map(|f| f.data)
        } else {
            None
        }
    }

    fn get_dir_files(&self, path: &Path) -> Vec<(PathBuf, std::borrow::Cow<'static, [u8]>)> {
        let prefix = path.to_str().unwrap_or("");
        if prefix.starts_with(KIBANA_ASSETS_DIR) {
            return get_kibana_bundle_dir_files(prefix);
        }

        let mut files: Vec<_> = Assets::iter()
            .filter(|p| p.starts_with(prefix))
            .filter_map(|p| {
                let p_str = p.as_ref();
                let p_buf = PathBuf::from(p_str);
                Assets::get(p_str).map(|f| (p_buf, f.data))
            })
            .collect();
        files.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
        files
    }
}

fn open_kibana_bundle() -> Option<ZipArchive<Cursor<&'static [u8]>>> {
    ZipArchive::new(Cursor::new(KIBANA_ASSETS_BUNDLE)).ok()
}

fn get_kibana_bundle_file(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    let mut archive = open_kibana_bundle()?;
    let mut file = archive.by_name(path).ok()?;
    let mut contents = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut contents).ok()?;
    Some(std::borrow::Cow::Owned(contents))
}

fn get_kibana_bundle_dir_files(prefix: &str) -> Vec<(PathBuf, std::borrow::Cow<'static, [u8]>)> {
    let Some(mut archive) = open_kibana_bundle() else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut file) = archive.by_index(i) else {
            continue;
        };
        if !file.is_file() || !file.name().starts_with(prefix) {
            continue;
        }

        let mut contents = Vec::with_capacity(file.size() as usize);
        if file.read_to_end(&mut contents).is_ok() {
            files.push((PathBuf::from(file.name()), std::borrow::Cow::Owned(contents)));
        }
    }

    files.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
    files
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Asset {
    pub endpoint: String,
    pub method: String,
    pub name: String,
    #[serde(default = "default_headers")]
    pub headers: HashMap<String, String>,
    pub suffix: Option<String>,
    pub query: Option<String>,
    #[serde(default)]
    pub requires_security: bool,
}

fn default_headers() -> HashMap<String, String> {
    HashMap::from([("Content-Type".to_string(), "application/json".to_string())])
}

fn should_skip_asset(asset: &Asset, security_enabled: bool) -> bool {
    asset.requires_security && !security_enabled
}

async fn send_asset(client: &Client, asset: &Asset, path: &Path, contents: &[u8], named: bool) -> Result<()> {
    send_asset_with_allowed_statuses(client, asset, path, contents, named, &[]).await
}

async fn send_asset_with_allowed_statuses(
    client: &Client,
    asset: &Asset,
    path: &Path,
    contents: &[u8],
    named: bool,
    allowed_statuses: &[StatusCode],
) -> Result<()> {
    let stem = path.file_stem().unwrap().to_str().unwrap_or("");
    let endpoint = match named {
        true => &format!(
            "{}/{}{}",
            &asset.endpoint,
            &stem,
            asset.suffix.clone().unwrap_or("".to_string()),
        ),
        false => &asset.endpoint,
    };
    match client
        .request(asset.method.parse()?, &asset.headers, endpoint, Some(contents))
        .await
    {
        Ok(response) => {
            let status = response.status();
            match status.is_success() || allowed_statuses.contains(&status) {
                true => {
                    let body = response.text().await?;
                    tracing::info!("{} {} {} {}", &asset.name, &stem, &asset.method, status);
                    tracing::trace!("Response body: {}", body);
                    Ok(())
                }
                false => {
                    let bytes = response.bytes().await?;
                    let body = serde_json::from_slice::<Value>(&bytes)?;
                    let message = format!("Asset: {body}");
                    Err(eyre!(message))
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to send asset: {e:?}");
            Err(eyre!(e))
        }
    }
}

/// Submit saved assets to the client APIs
pub async fn assets(client: &Client) -> Result<()> {
    let embedded_assets = EmbeddedAssets::new()?;
    if Product::from(client) == Product::Kibana {
        return kibana_assets(client, &embedded_assets).await;
    }

    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into(), &embedded_assets)?;

    // Check security status
    let security_enabled = client
        .has_security_enabled()
        .await
        .wrap_err("Failed to determine security status")?;

    if !security_enabled {
        tracing::info!("Security is disabled on the cluster. Security-dependent assets will be skipped.");
    }

    let mut error_count = 0;

    for asset in assets {
        if should_skip_asset(&asset, security_enabled) {
            tracing::debug!("Skipping security-dependent asset: {}", &asset.name);
            continue;
        }

        tracing::info!("Processing asset: {}", &asset.name);
        tracing::debug!("Asset: {:?}", &asset);
        let path = PathBuf::from(format!("{}/{}", client, asset.name));

        let dir_files = embedded_assets.get_dir_files(&path);
        if !dir_files.is_empty() {
            // do something with the directory
            for (file_path, contents) in dir_files {
                tracing::debug!("file.path: {:?}", file_path);
                match send_asset(client, &asset, &file_path, &contents, true).await {
                    Ok(res) => tracing::debug!("Response: {:?}", res),
                    Err(e) => {
                        tracing::error!("Failed to send asset: {e:?}");
                        error_count += 1;
                    }
                }
            }
        } else if let Some(contents) = embedded_assets.get_file(&path) {
            // do something with the file
            tracing::debug!("file.path: {:?}", &path);
            if let Err(e) = send_asset(client, &asset, &path, &contents, false).await {
                tracing::error!("Failed to send asset: {e:?}");
                error_count += 1;
            }
        } else {
            tracing::error!("Asset not found: {}", &asset.name);
            return Err(eyre!("Asset not found: {}", asset.name));
        }
    }
    if error_count == 0 {
        tracing::info!("completed setup for {client}");
        Ok(())
    } else {
        tracing::error!("{error_count} errors in setup for {client}");
        Err(eyre!("{error_count} errors in setup for {client}"))
    }
}

/// Start and verify a trial license before loading Enterprise-only Kibana assets.
pub async fn ensure_agent_builder_license(client: &Client) -> Result<()> {
    let Client::Elasticsearch(_) = client else {
        return Err(eyre!("an Elasticsearch client is required to start the trial license"));
    };

    if agent_builder_license_is_active(&current_license(client).await?) {
        return Ok(());
    }

    tracing::info!("Starting Elasticsearch trial license for Kibana Agent Builder assets");
    let response = client
        .request(
            Method::POST,
            &HashMap::new(),
            "_license/start_trial?acknowledge=true",
            None,
        )
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to start Elasticsearch trial license ({status}): {}",
            response.text().await?
        ));
    }

    if agent_builder_license_is_active(&current_license(client).await?) {
        Ok(())
    } else {
        Err(eyre!(
            "Elasticsearch trial license did not become active; Kibana Agent Builder assets will not be loaded"
        ))
    }
}

async fn current_license(client: &Client) -> Result<Value> {
    let response = client.request(Method::GET, &HashMap::new(), "_license", None).await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to read Elasticsearch license ({status}): {}",
            response.text().await?
        ));
    }
    Ok(response.json().await?)
}

fn agent_builder_license_is_active(response: &Value) -> bool {
    let license = response.get("license").unwrap_or(response);
    license.get("status").and_then(Value::as_str) == Some("active")
        && matches!(
            license.get("type").and_then(Value::as_str),
            Some("trial" | "enterprise")
        )
}

async fn kibana_assets(client: &Client, embedded_assets: &EmbeddedAssets) -> Result<()> {
    let bundle = kibana_bundle(embedded_assets)?.read_all()?;
    let Client::Kibana(kibana) = client else {
        return Err(eyre!("expected Kibana client"));
    };
    let spaces = bundle
        .spaces
        .iter()
        .filter_map(|space| {
            Some((
                space.get("id")?.as_str()?.to_string(),
                space.get("name")?.as_str()?.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    let sync_client = kibana.sync_client(spaces)?;

    let saved_objects_bundle = saved_objects_bundle(&bundle);
    let saved_objects_summary = push_sync(&sync_client, &saved_objects_bundle, &SyncOptions::default()).await?;
    ensure_sync_completed(&saved_objects_bundle, &saved_objects_summary)?;

    let agent_builder_bundle = agent_builder_bundle(bundle);
    for asset_kind in [
        KibanaAssetKind::Workflows,
        KibanaAssetKind::Tools,
        KibanaAssetKind::Skills,
        KibanaAssetKind::Agents,
    ] {
        let asset_bundle = kibana_asset_bundle(&agent_builder_bundle, asset_kind);
        let summary = push_sync(&sync_client, &asset_bundle, &SyncOptions::default()).await?;
        ensure_sync_completed(&asset_bundle, &summary)?;
    }

    for (space_id, space_bundle) in &agent_builder_bundle.by_space {
        let skill_ids = space_bundle
            .skills
            .iter()
            .filter_map(|skill| skill.get("id").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        attach_skills_to_default_agent(client, space_id, &skill_ids).await?;
    }

    tracing::info!("completed setup for {client}");
    Ok(())
}

fn saved_objects_bundle(bundle: &SyncBundle) -> SyncBundle {
    let mut saved_objects_bundle = bundle.clone();
    for space_bundle in saved_objects_bundle.by_space.values_mut() {
        space_bundle.workflows.clear();
        space_bundle.agents.clear();
        space_bundle.tools.clear();
        space_bundle.skills.clear();
    }
    saved_objects_bundle
}

fn agent_builder_bundle(mut bundle: SyncBundle) -> SyncBundle {
    bundle.spaces.clear();
    for space_bundle in bundle.by_space.values_mut() {
        space_bundle.saved_objects.clear();
    }
    bundle
}

#[derive(Clone, Copy)]
enum KibanaAssetKind {
    Workflows,
    Tools,
    Skills,
    Agents,
}

fn kibana_asset_bundle(bundle: &SyncBundle, asset_kind: KibanaAssetKind) -> SyncBundle {
    let mut asset_bundle = bundle.clone();
    asset_bundle.spaces.clear();
    for space_bundle in asset_bundle.by_space.values_mut() {
        space_bundle.saved_objects.clear();
        match asset_kind {
            KibanaAssetKind::Workflows => {
                space_bundle.agents.clear();
                space_bundle.tools.clear();
                space_bundle.skills.clear();
            }
            KibanaAssetKind::Tools => {
                space_bundle.workflows.clear();
                space_bundle.agents.clear();
                space_bundle.skills.clear();
            }
            KibanaAssetKind::Skills => {
                space_bundle.workflows.clear();
                space_bundle.agents.clear();
                space_bundle.tools.clear();
            }
            KibanaAssetKind::Agents => {
                space_bundle.workflows.clear();
                space_bundle.tools.clear();
                space_bundle.skills.clear();
            }
        }
    }
    asset_bundle
}

fn ensure_sync_completed(bundle: &SyncBundle, summary: &SyncSummary) -> Result<()> {
    let expected_saved_objects = bundle
        .by_space
        .values()
        .map(|space| space.saved_objects.len())
        .sum::<usize>();
    let expected_workflows = bundle
        .by_space
        .values()
        .map(|space| space.workflows.len())
        .sum::<usize>();
    let expected_agents = bundle.by_space.values().map(|space| space.agents.len()).sum::<usize>();
    let expected_tools = bundle.by_space.values().map(|space| space.tools.len()).sum::<usize>();
    let expected_skills = bundle.by_space.values().map(|space| space.skills.len()).sum::<usize>();
    let complete = summary.spaces_applied == bundle.spaces.len()
        && summary.saved_objects_applied == expected_saved_objects
        && summary.workflows_applied == expected_workflows
        && summary.agents_applied == expected_agents
        && summary.tools_applied == expected_tools
        && summary.skills_applied == expected_skills;
    if complete {
        Ok(())
    } else {
        Err(eyre!(
            "Kibana sync was incomplete: spaces {}/{}, saved objects {}/{}, workflows {}/{}, agents {}/{}, tools {}/{}, skills {}/{}",
            summary.spaces_applied,
            bundle.spaces.len(),
            summary.saved_objects_applied,
            expected_saved_objects,
            summary.workflows_applied,
            expected_workflows,
            summary.agents_applied,
            expected_agents,
            summary.tools_applied,
            expected_tools,
            summary.skills_applied,
            expected_skills,
        ))
    }
}

fn kibana_bundle(assets_store: &EmbeddedAssets) -> Result<KibanaBundle<kibana_sync::Entries<Vec<u8>>>> {
    let root = Path::new(KIBANA_ASSETS_DIR);
    let entries = assets_store
        .get_dir_files(root)
        .into_iter()
        .map(|(path, contents)| {
            let path = path
                .strip_prefix(root)
                .wrap_err_with(|| format!("Kibana asset is outside embedded bundle root: {}", path.display()))?
                .to_path_buf();
            Ok((path, contents.into_owned()))
        })
        .collect::<Result<Vec<_>>>()?;
    KibanaBundle::from_entries(entries).map_err(Into::into)
}

async fn attach_skills_to_default_agent(client: &Client, space_id: &str, skill_ids: &[String]) -> Result<()> {
    if skill_ids.is_empty() {
        return Ok(());
    }
    let path = format!("s/{space_id}/api/agent_builder/agents/elastic-ai-agent");
    let response = client.request(Method::GET, &HashMap::new(), &path, None).await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to read Kibana default agent ({status}): {}",
            response.text().await?
        ));
    }
    let mut agent: Value = response.json().await?;
    let configuration = agent
        .get_mut("configuration")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| eyre!("Kibana default agent did not include configuration"))?;
    let configured_skills = configuration
        .entry("skill_ids")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| eyre!("Kibana default agent skill_ids was not an array"))?;
    for skill_id in skill_ids {
        if !configured_skills.iter().any(|value| value.as_str() == Some(skill_id)) {
            configured_skills.push(Value::String(skill_id.clone()));
        }
    }
    if let Some(agent) = agent.as_object_mut() {
        agent.remove("id");
        agent.remove("readonly");
        agent.remove("type");
        agent.remove("created_by");
    }
    send_kibana_json_with_method(client, Method::PUT, &path, &agent, false).await
}

async fn send_kibana_json_with_method(
    client: &Client,
    method: Method,
    path: &str,
    value: &Value,
    internal: bool,
) -> Result<()> {
    let mut headers = default_headers();
    if internal {
        headers.insert("X-Elastic-Internal-Origin".to_string(), "Kibana".to_string());
    }
    let response = client
        .request(method, &headers, path, Some(&serde_json::to_vec(value)?))
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Kibana asset request failed with status {status}: {}",
            response.text().await?
        ));
    }
    Ok(())
}

/// Parses the assets YAML file for the given exporter. Currently only supports Elasticsearch.
fn parse_assets_yml(product: Product, assets_store: &EmbeddedAssets) -> Result<Vec<Asset>> {
    let filename = format!("{}/{}", product.to_string().to_lowercase(), ASSETS_FILE);
    let contents = assets_store
        .get_file(Path::new(&filename))
        .ok_or(eyre!("embedded assets did not contain expected file {filename}"))?;
    let assets = serde_yaml::from_slice(&contents)?;
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_deserialization_with_requires_security() {
        let yaml = r#"
- name: "roles"
  endpoint: "_security/role"
  method: "PUT"
  requires_security: true
- name: "ingest_pipelines"
  endpoint: "_ingest/pipeline"
  method: "PUT"
"#;
        let assets: Vec<Asset> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].name, "roles");
        assert!(assets[0].requires_security);
        assert_eq!(assets[1].name, "ingest_pipelines");
        assert!(!assets[1].requires_security);
    }

    #[test]
    fn test_should_skip_asset() {
        let security_asset = Asset {
            endpoint: "/".to_string(),
            method: "GET".to_string(),
            name: "test".to_string(),
            headers: HashMap::new(),
            suffix: None,
            query: None,
            requires_security: true,
        };
        let normal_asset = Asset {
            endpoint: "/".to_string(),
            method: "GET".to_string(),
            name: "test".to_string(),
            headers: HashMap::new(),
            suffix: None,
            query: None,
            requires_security: false,
        };

        // Security enabled: skip nothing
        assert!(!should_skip_asset(&security_asset, true));
        assert!(!should_skip_asset(&normal_asset, true));

        // Security disabled: skip security asset
        assert!(should_skip_asset(&security_asset, false));
        assert!(!should_skip_asset(&normal_asset, false));
    }

    #[test]
    fn kibana_assets_follow_kibana_sync_bundle_layout() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        assert_eq!(bundle.spaces.len(), 1);
        assert_eq!(bundle.spaces[0]["id"], "esdiag");

        let esdiag = bundle.by_space.get("esdiag").unwrap();
        assert_eq!(esdiag.saved_objects.len(), 90);
        assert_eq!(esdiag.workflows.len(), 1);
        assert!(esdiag.agents.is_empty());
        assert_eq!(esdiag.tools.len(), 1);
        assert_eq!(esdiag.skills.len(), 1);
        assert!(esdiag.skills[0]["referenced_content"].as_array().unwrap().len() > 1);
    }

    #[test]
    fn agent_builder_license_requires_an_active_trial_or_enterprise_license() {
        assert!(agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "trial"}
        })));
        assert!(agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "enterprise"}
        })));
        assert!(!agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "basic"}
        })));
        assert!(!agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "expired", "type": "trial"}
        })));
    }

    #[test]
    fn kibana_sync_phases_keep_saved_objects_license_independent() {
        let mut bundle = SyncBundle::default();
        bundle
            .spaces
            .push(serde_json::json!({"id": "esdiag", "name": "esdiag"}));
        bundle.by_space.insert(
            "esdiag".to_string(),
            kibana_sync::sync::SpaceBundle {
                saved_objects: vec![serde_json::json!({"id": "dashboard-1"})],
                workflows: vec![serde_json::json!({"id": "workflow-1"})],
                tools: vec![serde_json::json!({"id": "tool-1"})],
                skills: vec![serde_json::json!({"id": "skill-1"})],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );

        let saved_objects = saved_objects_bundle(&bundle);
        let agent_builder = agent_builder_bundle(bundle);

        assert_eq!(saved_objects.spaces.len(), 1);
        assert_eq!(saved_objects.by_space["esdiag"].saved_objects.len(), 1);
        assert!(saved_objects.by_space["esdiag"].tools.is_empty());
        assert!(agent_builder.spaces.is_empty());
        assert!(agent_builder.by_space["esdiag"].saved_objects.is_empty());
        assert_eq!(agent_builder.by_space["esdiag"].workflows.len(), 1);
        assert_eq!(agent_builder.by_space["esdiag"].tools.len(), 1);
        assert_eq!(agent_builder.by_space["esdiag"].skills.len(), 1);

        let workflows = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Workflows);
        let tools = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Tools);
        let skills = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Skills);
        assert_eq!(workflows.by_space["esdiag"].workflows.len(), 1);
        assert!(workflows.by_space["esdiag"].tools.is_empty());
        assert_eq!(tools.by_space["esdiag"].tools.len(), 1);
        assert!(tools.by_space["esdiag"].skills.is_empty());
        assert_eq!(skills.by_space["esdiag"].skills.len(), 1);
        assert!(skills.by_space["esdiag"].tools.is_empty());
    }

    #[test]
    fn incomplete_kibana_sync_prevents_default_agent_updates() {
        let mut bundle = SyncBundle::default();
        bundle.by_space.insert(
            "esdiag".to_string(),
            kibana_sync::sync::SpaceBundle {
                skills: vec![serde_json::json!({"id": "skill-1"})],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );

        let error = ensure_sync_completed(&bundle, &SyncSummary::default()).unwrap_err();
        assert!(error.to_string().contains("skills 0/1"));
    }

    #[test]
    fn kibana_assets_are_embedded_as_bundle_not_raw_files() {
        assert!(KIBANA_ASSETS_BUNDLE.len() > 0);
        assert!(Assets::get("kibana/spaces.yml").is_none());

        let embedded_assets = EmbeddedAssets::new().unwrap();
        let spaces = embedded_assets
            .get_file(Path::new("kibana/spaces.yml"))
            .expect("Kibana spaces manifest should load from bundle");

        assert!(std::str::from_utf8(&spaces).unwrap().contains("id: esdiag"));
    }

    #[test]
    fn kibana_bundle_uses_the_spaces_manifest() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        assert_eq!(bundle.spaces.len(), 1);
        assert_eq!(bundle.spaces[0]["id"], "esdiag");
        assert_eq!(bundle.spaces[0]["name"], "esdiag");
        assert_eq!(bundle.spaces[0]["description"], "Elastic Stack Diagnostics");
        assert_eq!(bundle.spaces[0]["solution"], "oblt");
    }

    #[test]
    fn kibana_sync_resolves_saved_objects_from_display_name_files() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();
        let saved_objects = &bundle.by_space["esdiag"].saved_objects;

        assert_eq!(saved_objects.len(), 90);
        assert_eq!(saved_objects[0]["type"], "dashboard");
        assert_eq!(saved_objects[0]["id"], "allocation-overview");
    }

    #[test]
    fn kibana_sync_preserves_saved_object_json_string_fields() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        for object in &bundle.by_space["esdiag"].saved_objects {
            let label = saved_object_label(&object);
            let attributes = object
                .get("attributes")
                .unwrap_or_else(|| panic!("{label} should have attributes"));

            assert_json_string_fields_parse(&label, attributes);
            assert_vega_spec_parses(&label, attributes);
        }
    }

    #[test]
    fn kibana_readme_dashboard_links_to_esdiag_issues() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();
        let readme = bundle.by_space["esdiag"]
            .saved_objects
            .iter()
            .find(|object| object["id"] == "esdiag-readme")
            .expect("readme dashboard should be embedded");
        let content = serde_json::to_string(readme).unwrap();

        assert!(content.contains("https://github.com/elastic/esdiag/issues"));
        assert!(!content.contains("https://github.com/elastic/issues)"));
    }

    fn saved_object_label(object: &Value) -> String {
        format!(
            "{}/{}",
            object["type"].as_str().unwrap_or("<missing-type>"),
            object["id"].as_str().unwrap_or("<missing-id>")
        )
    }

    fn assert_json_string_fields_parse(label: &str, value: &Value) {
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    if let Some(text) = child.as_str()
                        && (key == "visState" || key.ends_with("JSON"))
                    {
                        serde_json::from_str::<Value>(text)
                            .unwrap_or_else(|err| panic!("{label}.{key} should parse as JSON: {err}"));
                    }
                    assert_json_string_fields_parse(label, child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    assert_json_string_fields_parse(label, child);
                }
            }
            _ => {}
        }
    }

    fn assert_vega_spec_parses(label: &str, attributes: &Value) {
        let Some(vis_state) = attributes.get("visState").and_then(Value::as_str) else {
            return;
        };
        let vis_state: Value = serde_json::from_str(vis_state)
            .unwrap_or_else(|err| panic!("{label}.visState should parse as JSON: {err}"));

        if vis_state["type"].as_str() != Some("vega") {
            return;
        }

        let spec = vis_state["params"]["spec"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}.visState.params.spec should be a string"));
        serde_json::from_str::<Value>(spec)
            .unwrap_or_else(|err| panic!("{label}.visState.params.spec should parse as JSON: {err}"));
    }
}
