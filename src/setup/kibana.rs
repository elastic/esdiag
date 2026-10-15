// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::*;
use crate::client::Client;
//use bytes::Bytes;
use eyre::{Result, WrapErr, eyre};
use kibana_sync::{
    KibanaBundle,
    sync::{SyncBundle, SyncOptions, SyncSummary, push_sync},
};
use reqwest::{Method, StatusCode};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) async fn kibana_assets(
    client: &Client,
    embedded_assets: &EmbeddedAssets,
    space: Option<String>,
) -> Result<()> {
    let mut bundle = kibana_bundle(embedded_assets)?.read_all()?;
    ensure_single_kibana_space(client, &bundle, space.as_deref().unwrap_or("default")).await?;
    target_kibana_bundle(&mut bundle, space.as_deref())?;
    if client.is_serverless().await? {
        adapt_serverless_kibana_bundle(&mut bundle);
    }
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
    sync_kibana_bundle(&sync_client, &bundle).await?;

    let agent_builder_bundle = agent_builder_bundle(bundle);
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

/// Saved object IDs are not space safe and workflow IDs are global, so ESDiag
/// Kibana assets may only live in one space per deployment.
pub(super) async fn ensure_single_kibana_space(client: &Client, bundle: &SyncBundle, target: &str) -> Result<()> {
    let dashboard_ids = bundled_dashboard_ids(bundle);
    if dashboard_ids.is_empty() {
        return Ok(());
    }
    let response = client
        .request(Method::GET, &HashMap::new(), "api/spaces/space", None)
        .await?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        return Ok(());
    }
    if !status.is_success() {
        return Err(eyre!(
            "Failed to list Kibana spaces ({status}): {}",
            response.text().await?
        ));
    }
    let spaces: Vec<Value> = response.json().await?;
    for space in spaces
        .iter()
        .filter_map(|space| space.get("id").and_then(Value::as_str))
        .filter(|space| *space != target)
    {
        if space_has_esdiag_dashboards(client, space, &dashboard_ids).await? {
            let setting = if space == "default" { "_default" } else { space };
            return Err(eyre!(
                "ESDiag Kibana assets are already installed in the '{space}' space. \
                 ESDiag assets can only be installed in one Kibana space: set \
                 ESDIAG_KIBANA_SPACE={setting} to update that install, or remove the \
                 ESDiag assets from '{space}' before installing into '{target}'."
            ));
        }
    }
    Ok(())
}

pub(super) fn bundled_dashboard_ids(bundle: &SyncBundle) -> HashSet<String> {
    bundle
        .by_space
        .values()
        .flat_map(|assets| &assets.saved_objects)
        .filter(|object| object.get("type").and_then(Value::as_str) == Some("dashboard"))
        .filter_map(|object| object.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

pub(super) async fn space_has_esdiag_dashboards(
    client: &Client,
    space: &str,
    dashboard_ids: &HashSet<String>,
) -> Result<bool> {
    const PER_PAGE: u64 = 1000;
    for page in 1.. {
        let path = kibana_space_path(
            space,
            &format!("api/saved_objects/_find?type=dashboard&fields=title&per_page={PER_PAGE}&page={page}"),
        );
        let response = client.request(Method::GET, &HashMap::new(), &path, None).await?;
        let status = response.status();
        if !status.is_success() {
            return Err(eyre!(
                "Failed to search Kibana space '{space}' for ESDiag assets ({status}): {}",
                response.text().await?
            ));
        }
        let body: Value = response.json().await?;
        if contains_esdiag_saved_object(&body, dashboard_ids) {
            return Ok(true);
        }
        let total = body.get("total").and_then(Value::as_u64).unwrap_or(0);
        if page * PER_PAGE >= total {
            break;
        }
    }
    Ok(false)
}

/// Kibana regenerates saved object IDs when an existing ID is imported into
/// another space and keeps the original in `originId`.
pub(super) fn contains_esdiag_saved_object(find_response: &Value, ids: &HashSet<String>) -> bool {
    find_response
        .get("saved_objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|object| [object.get("id"), object.get("originId")])
        .flatten()
        .filter_map(Value::as_str)
        .any(|id| ids.contains(id))
}

pub(super) async fn sync_kibana_bundle(client: &kibana_sync::KibanaClient, bundle: &SyncBundle) -> Result<()> {
    let mut summary = SyncSummary::default();
    accumulate_sync_result(
        bundle,
        &mut summary,
        push_sync(client, &saved_objects_bundle(bundle), &SyncOptions::default()).await,
    )?;

    let agent_builder_bundle = agent_builder_bundle(bundle.clone());
    for asset_kind in [
        KibanaAssetKind::Workflows,
        KibanaAssetKind::Tools,
        KibanaAssetKind::Skills,
        KibanaAssetKind::Agents,
    ] {
        let asset_bundle = kibana_asset_bundle(&agent_builder_bundle, asset_kind);
        accumulate_sync_result(
            bundle,
            &mut summary,
            push_sync(client, &asset_bundle, &SyncOptions::default()).await,
        )?;
    }
    ensure_sync_completed(bundle, &summary)
}

pub(super) fn target_kibana_bundle(bundle: &mut SyncBundle, space: Option<&str>) -> Result<()> {
    let target = space.unwrap_or("default");
    if target == "esdiag" {
        return Ok(());
    }
    if bundle.by_space.len() != 1 || !bundle.by_space.contains_key("esdiag") {
        return Err(eyre!(
            "Expected a single esdiag asset space before selecting a destination"
        ));
    }
    let mut assets = bundle.by_space.remove("esdiag").expect("checked asset space");
    let prefix = space
        .map(|s| format!("/s/{}", urlencoding::encode(s)))
        .unwrap_or_default();
    for value in assets
        .saved_objects
        .iter_mut()
        .chain(&mut assets.workflows)
        .chain(&mut assets.agents)
        .chain(&mut assets.tools)
        .chain(&mut assets.skills)
    {
        rewrite_kibana_asset_links(value, &prefix);
    }
    bundle.by_space.insert(target.to_string(), assets);
    if space.is_none() {
        bundle.spaces.clear();
    } else {
        for definition in &mut bundle.spaces {
            definition["id"] = Value::String(target.to_string());
            definition["name"] = Value::String(target.to_string());
        }
    }
    Ok(())
}

pub(super) fn rewrite_kibana_asset_links(value: &mut Value, prefix: &str) {
    match value {
        Value::String(text) => *text = text.replace("/s/esdiag/", &format!("{prefix}/")),
        Value::Array(values) => values.iter_mut().for_each(|v| rewrite_kibana_asset_links(v, prefix)),
        Value::Object(values) => values.values_mut().for_each(|v| rewrite_kibana_asset_links(v, prefix)),
        _ => {}
    }
}

pub(super) fn adapt_serverless_kibana_bundle(bundle: &mut SyncBundle) {
    for space in &mut bundle.spaces {
        if let Some(space) = space.as_object_mut() {
            // Serverless fixes the solution and disallows space feature visibility controls.
            space.remove("solution");
            space.remove("disabledFeatures");
        }
    }
}

pub(super) fn saved_objects_bundle(bundle: &SyncBundle) -> SyncBundle {
    let mut saved_objects_bundle = bundle.clone();
    for space_bundle in saved_objects_bundle.by_space.values_mut() {
        space_bundle.workflows.clear();
        space_bundle.agents.clear();
        space_bundle.tools.clear();
        space_bundle.skills.clear();
    }
    saved_objects_bundle
}

pub(super) fn agent_builder_bundle(mut bundle: SyncBundle) -> SyncBundle {
    bundle.spaces.clear();
    for space_bundle in bundle.by_space.values_mut() {
        space_bundle.saved_objects.clear();
    }
    bundle
}

#[derive(Clone, Copy)]
pub(super) enum KibanaAssetKind {
    Workflows,
    Tools,
    Skills,
    Agents,
}

pub(super) fn kibana_asset_bundle(bundle: &SyncBundle, asset_kind: KibanaAssetKind) -> SyncBundle {
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

pub(super) fn accumulate_sync_summary(total: &mut SyncSummary, summary: SyncSummary) {
    total.spaces_attempted += summary.spaces_attempted;
    total.spaces_applied += summary.spaces_applied;
    total.saved_objects_attempted += summary.saved_objects_attempted;
    total.saved_objects_applied += summary.saved_objects_applied;
    total.workflows_attempted += summary.workflows_attempted;
    total.workflows_applied += summary.workflows_applied;
    total.agents_attempted += summary.agents_attempted;
    total.agents_applied += summary.agents_applied;
    total.tools_attempted += summary.tools_attempted;
    total.tools_applied += summary.tools_applied;
    total.skills_attempted += summary.skills_attempted;
    total.skills_applied += summary.skills_applied;
}

pub(super) fn accumulate_sync_result<E>(
    bundle: &SyncBundle,
    total: &mut SyncSummary,
    result: std::result::Result<SyncSummary, E>,
) -> Result<()>
where
    E: std::error::Error + Send + Sync + 'static,
{
    match result {
        Ok(summary) => {
            accumulate_sync_summary(total, summary);
            Ok(())
        }
        Err(error) => Err(eyre::Report::new(error).wrap_err(sync_progress(bundle, total))),
    }
}

pub(super) fn sync_progress(bundle: &SyncBundle, summary: &SyncSummary) -> String {
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
    format!(
        "Kibana sync failed after applying spaces {}/{}, saved objects {}/{}, workflows {}/{}, agents {}/{}, tools {}/{}, skills {}/{}",
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
    )
}

pub(super) fn ensure_sync_completed(bundle: &SyncBundle, summary: &SyncSummary) -> Result<()> {
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

pub(super) fn kibana_bundle(assets_store: &EmbeddedAssets) -> Result<KibanaBundle<kibana_sync::Entries<Vec<u8>>>> {
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

pub(super) async fn attach_skills_to_default_agent(
    client: &Client,
    space_id: &str,
    skill_ids: &[String],
) -> Result<()> {
    if skill_ids.is_empty() {
        return Ok(());
    }
    let path = default_agent_path(space_id);
    let response = client.request(Method::GET, &HashMap::new(), &path, None).await?;
    let status = response.status();
    if !status.is_success() {
        return Err(eyre!(
            "Failed to read Kibana default agent ({status}): {}",
            response.text().await?
        ));
    }
    let agent: Value = response.json().await?;
    let update = default_agent_skill_update(agent, skill_ids)?;
    send_kibana_json_with_method(client, Method::PUT, &path, &update, false).await
}

pub(super) fn default_agent_path(space_id: &str) -> String {
    kibana_space_path(space_id, "api/agent_builder/agents/elastic-ai-agent")
}

pub(super) fn kibana_space_path(space_id: &str, endpoint: &str) -> String {
    if space_id == "default" {
        endpoint.to_string()
    } else {
        format!("s/{}/{endpoint}", urlencoding::encode(space_id))
    }
}

pub(super) fn default_agent_skill_update(mut agent: Value, skill_ids: &[String]) -> Result<Value> {
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
    // The update API accepts a partial body. Read responses also contain server-owned
    // fields such as access_control.entries that must not be echoed into an update.
    Ok(serde_json::json!({ "configuration": configuration }))
}

pub(super) async fn send_kibana_json_with_method(
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
