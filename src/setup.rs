// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    client::Client,
    data::Application,
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
use url::Url;
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

/// Every ESDiag-owned index, whichever generation created it.
const ESDIAG_INDEX_PATTERN: &str = "*-esdiag*";

/// The renamed `diagnostic.*` provenance fields as `(current, legacy)` pairs
/// (ADR-0001, bridged by ADR-0014).
const PROVENANCE_RENAMES: [(&str, &str); 2] = [("application", "product"), ("platform", "orchestration")];

/// The alias mapping an index created before the provenance rename is missing.
///
/// A field alias must point at a concrete field, so no single index can carry
/// both names as aliases: the direction is fixed by which name that index stores.
/// New templates keep both names writable with reciprocal `copy_to`. Historical
/// indices still need a query-only alias because adding a concrete field would
/// leave previously indexed documents unsearchable through that name.
///
/// Returns `None` when there is nothing to bridge — a new-generation index, one
/// already carrying the alias, or a mapping with no provenance fields at all —
/// which is what makes running this on every setup idempotent.
fn provenance_alias_patch(diagnostic_properties: &Value) -> Option<Value> {
    let mut aliases = serde_json::Map::new();
    for (current, legacy) in PROVENANCE_RENAMES {
        let stores_legacy_name = diagnostic_properties
            .get(legacy)
            .and_then(|mapping| mapping.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|field_type| field_type != "alias");
        if stores_legacy_name && diagnostic_properties.get(current).is_none() {
            aliases.insert(
                current.to_string(),
                serde_json::json!({ "type": "alias", "path": format!("diagnostic.{legacy}") }),
            );
        }
    }

    if aliases.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "properties": { "diagnostic": { "properties": Value::Object(aliases) } }
    }))
}

/// The indices needing a provenance alias, paired with the mapping patch each one
/// needs, read from a `GET <pattern>/_mapping` response.
fn provenance_alias_patches(mappings: &Value) -> Vec<(String, Value)> {
    let Some(indices) = mappings.as_object() else {
        return Vec::new();
    };
    indices
        .iter()
        .filter_map(|(index, mapping)| {
            let properties = mapping
                .pointer("/mappings/properties/diagnostic/properties")
                .unwrap_or(&Value::Null);
            provenance_alias_patch(properties).map(|patch| (index.clone(), patch))
        })
        .collect()
}

/// Existing aliases cannot accept writes, and unlinked concrete fields can split
/// search results. Neither can be repaired by installing a new template.
fn provenance_mapping_warnings(properties: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    for (current, legacy) in PROVENANCE_RENAMES {
        for name in [current, legacy] {
            if properties[name]["type"] == "alias" {
                warnings.push(format!(
                    "diagnostic.{name} is a query-only alias and rejects writers using that name; \
                     roll over the data stream after setup before mixing writer versions"
                ));
            }
        }
        let concrete = |name: &str| properties[name]["type"].as_str().is_some_and(|kind| kind != "alias");
        let copies_to = |from: &str, to: &str| {
            let target = Value::String(format!("diagnostic.{to}"));
            let copy = &properties[from]["copy_to"];
            copy == &target || copy.as_array().is_some_and(|targets| targets.contains(&target))
        };
        if concrete(current) && concrete(legacy) && !(copies_to(current, legacy) && copies_to(legacy, current)) {
            warnings.push(format!(
                "diagnostic.{current} and diagnostic.{legacy} are unlinked concrete fields; \
                 historical search results may be split. Roll over for future writes and \
                 reindex historical documents if both names must find them"
            ));
        }
    }
    warnings
}

/// Bridge the provenance rename on indices that predate it (ADR-0014).
///
/// Templates only govern indices created after they are installed, so this is the
/// other half of the bridge: without it a dashboard querying the current field
/// name silently matches nothing in historical indices. Failures are reported and
/// returned to the caller so the CLI can report partial installation while
/// allowing the remaining assets to be installed.
async fn install_provenance_aliases(client: &Client) -> Result<Vec<String>> {
    let headers = default_headers();
    let path = format!("/{ESDIAG_INDEX_PATTERN}/_mapping?ignore_unavailable=true&allow_no_indices=true");
    let response = client.request(Method::GET, &headers, &path, None).await?;
    if !response.status().is_success() {
        return Err(eyre!(
            "Reading ESDiag index mappings returned {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let mappings: Value = response.json().await?;
    if let Some(indices) = mappings.as_object() {
        for (index, mapping) in indices {
            let properties = mapping
                .pointer("/mappings/properties/diagnostic/properties")
                .unwrap_or(&Value::Null);
            for warning in provenance_mapping_warnings(properties) {
                tracing::warn!("{index}: {warning}");
            }
        }
    }
    let patches = provenance_alias_patches(&mappings);
    if patches.is_empty() {
        tracing::debug!("No pre-rename ESDiag indices need a provenance alias");
        return Ok(vec![]);
    }

    let mut failures = Vec::new();
    for (index, patch) in &patches {
        let body = serde_json::to_vec(patch)?;
        let result = client
            .request(Method::PUT, &headers, &format!("/{index}/_mapping"), Some(&body))
            .await;
        match result {
            Ok(response) if response.status().is_success() => {
                tracing::debug!("Aliased the current provenance names onto {index}");
            }
            Ok(response) => {
                failures.push(index.clone());
                tracing::warn!(
                    "Could not add the provenance alias to {index}: {} {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
            }
            Err(err) => {
                failures.push(index.clone());
                tracing::warn!("Could not add the provenance alias to {index}: {err}");
            }
        }
    }

    tracing::info!(
        "Bridged provenance field names on {} of {} pre-rename indices",
        patches.len() - failures.len(),
        patches.len()
    );
    if patches.len() > failures.len() {
        tracing::warn!(
            "Added query-only provenance aliases. Roll over the affected data streams after setup before mixing writer versions."
        );
    }
    Ok(failures)
}

fn should_skip_asset(asset: &Asset, security_assets_supported: bool) -> bool {
    asset.requires_security && !security_assets_supported
}

async fn send_asset(client: &Client, asset: &Asset, path: &Path, contents: &[u8], named: bool) -> Result<()> {
    send_asset_with_allowed_statuses(client, asset, path, contents, named, &[]).await
}

/// Adapt only template settings. ILM fields in diagnostic mappings are source data.
fn serverless_asset_contents(asset: &Asset, contents: &[u8]) -> Result<Vec<u8>> {
    if !is_template_asset(asset) {
        return Ok(contents.to_vec());
    }
    let mut body: Value = serde_json::from_slice(contents)?;
    if let Some(settings) = body.pointer_mut("/template/settings") {
        remove_serverless_ilm_settings(settings, "");
    }
    // Serverless requires lifecycle management even for long-lived reports.
    if let Some(lifecycle) = body.pointer_mut("/template/lifecycle")
        && lifecycle.get("enabled") == Some(&Value::Bool(false))
    {
        *lifecycle = serde_json::json!({"enabled": true, "data_retention": "3650d"});
    }
    Ok(serde_json::to_vec(&body)?)
}

fn is_template_asset(asset: &Asset) -> bool {
    matches!(
        asset.endpoint.trim_matches('/'),
        "_component_template" | "_index_template"
    )
}

fn remove_serverless_ilm_settings(value: &mut Value, prefix: &str) {
    if let Some(settings) = value.as_object_mut() {
        settings.retain(|key, value| {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if matches!(path.as_str(), "index.lifecycle.name" | "index.lifecycle.prefer_ilm") {
                return false;
            }
            remove_serverless_ilm_settings(value, &path);
            !value.as_object().is_some_and(|object| object.is_empty())
        });
    }
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
            asset.endpoint,
            stem,
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

/// Mapping updates that could not be completed after asset installation.
#[derive(Default)]
pub struct SetupReport {
    pub failed_indices: Vec<String>,
    pub warnings: Vec<String>,
}

impl SetupReport {
    pub fn is_complete(&self) -> bool {
        self.failed_indices.is_empty() && self.warnings.is_empty()
    }
}

/// Install assets, requiring complete mapping updates before returning success.
pub async fn assets(client: &Client) -> Result<()> {
    let report = assets_report(client).await?;
    if !report.is_complete() {
        let failed_indices = if report.failed_indices.is_empty() {
            String::new()
        } else {
            format!(
                " These indices could not be updated: {}.",
                report.failed_indices.join(", ")
            )
        };
        let warnings = if report.warnings.is_empty() {
            String::new()
        } else {
            format!(" {}", report.warnings.join(" "))
        };
        return Err(eyre!("Asset installation was partial.{}{}", failed_indices, warnings));
    }
    Ok(())
}

pub async fn assets_report(client: &Client) -> Result<SetupReport> {
    let mut report = SetupReport::default();
    let embedded_assets = EmbeddedAssets::new()?;
    if Application::from(client) == Application::Kibana {
        kibana_assets(client, &embedded_assets).await?;
        return Ok(report);
    }

    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into(), &embedded_assets)?;

    // Check security status
    let serverless = client.is_serverless().await?;
    let security_enabled = serverless
        || client
            .has_security_enabled()
            .await
            .wrap_err("Failed to determine security status")?;
    if serverless {
        tracing::info!(
            "Serverless security is always enabled. Skipping bundled security-dependent assets; configure project roles separately."
        );
    } else if !security_enabled {
        tracing::info!("Security is disabled on the cluster. Security-dependent assets will be skipped.");
    }
    let supports_security_assets = security_enabled && !serverless;

    let mut error_count = 0;

    for asset in assets {
        if should_skip_asset(&asset, supports_security_assets) {
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
                let contents = if serverless && is_template_asset(&asset) {
                    std::borrow::Cow::Owned(serverless_asset_contents(&asset, &contents)?)
                } else {
                    contents
                };
                tracing::debug!("file.path: {:?}", file_path);
                match send_asset(client, &asset, &file_path, &contents, true).await {
                    Ok(res) => tracing::debug!("Response: {:?}", res),
                    Err(e) => {
                        tracing::error!("Failed to send asset: {e:?}");
                        report.warnings.push(format!(
                            "Failed to install {} asset {}. Check setup logs and rerun setup.",
                            client,
                            file_path.display()
                        ));
                        error_count += 1;
                    }
                }
            }
        } else if let Some(contents) = embedded_assets.get_file(&path) {
            let contents = if serverless && is_template_asset(&asset) {
                std::borrow::Cow::Owned(serverless_asset_contents(&asset, &contents)?)
            } else {
                contents
            };
            // do something with the file
            tracing::debug!("file.path: {:?}", &path);
            if let Err(e) = send_asset(client, &asset, &path, &contents, false).await {
                tracing::error!("Failed to send asset: {e:?}");
                report.warnings.push(format!(
                    "Failed to install {} asset {}. Check setup logs and rerun setup.",
                    client,
                    path.display()
                ));
                error_count += 1;
            }
        } else {
            tracing::error!("Asset not found: {}", &asset.name);
            return Err(eyre!("Asset not found: {}", asset.name));
        }
    }
    // The templates just installed only shape indices created from here on, so
    // existing ones still answer to whichever provenance names they were created
    // with (ADR-0014). Elasticsearch owns those indices, so the bridge is only
    // meaningful there — asking any other product for its mappings would warn
    // about a call that never made sense.
    if matches!(client, Client::Elasticsearch(_)) {
        match install_provenance_aliases(client).await {
            Ok(indices) => report.failed_indices = indices,
            Err(err) => {
                tracing::warn!("Could not bridge provenance field names on existing ESDiag indices: {err}");
                report.warnings.push("Could not inspect or update existing index mappings. Check connectivity and mapping privileges, then rerun setup.".to_string());
            }
        }
        if !report.failed_indices.is_empty() {
            report.warnings.push("Check the failed indices for mapping limits or write blocks. For a total-fields-limit error, raise index.mapping.total_fields.limit, then rerun setup.".to_string());
        }
    }

    if error_count == 0 {
        tracing::info!("finished asset installation for {client}");
        Ok(report)
    } else {
        tracing::error!("{error_count} errors in setup for {client}");
        Ok(report)
    }
}

/// Checks the minimum ESDiag asset set required for processing and Agent Builder.
///
/// This is deliberately read-only and inexpensive enough for onboarding status:
/// the Elasticsearch ingest pipeline proves processing assets are present, while
/// the default Kibana agent must have the ESDiag diagnostic skill attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssetStatus {
    Installed,
    Missing,
    #[default]
    Unknown,
}

pub async fn asset_statuses(
    elasticsearch: &Client,
    kibana: &Client,
    kibana_url: Option<&str>,
) -> (AssetStatus, AssetStatus) {
    let headers = HashMap::new();
    let pipeline = elasticsearch.request(Method::GET, &headers, "_ingest/pipeline/esdiag", None);
    let agent_path = kibana_agent_path(kibana_url);
    let agent = kibana.request(Method::GET, &headers, &agent_path, None);
    let (pipeline, agent) = tokio::join!(pipeline, agent);

    let elasticsearch_assets = match pipeline {
        Ok(response) if response.status().is_success() => AssetStatus::Installed,
        Ok(response) if response.status() == StatusCode::NOT_FOUND => AssetStatus::Missing,
        Ok(_) | Err(_) => AssetStatus::Unknown,
    };
    let kibana_assets = match agent {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(agent) => {
                let installed = agent
                    .get("configuration")
                    .and_then(|configuration| configuration.get("skill_ids"))
                    .and_then(Value::as_array)
                    .is_some_and(|skills| {
                        skills
                            .iter()
                            .any(|skill| skill.as_str() == Some("agentic-diagnostic-assistant"))
                    });
                if installed {
                    AssetStatus::Installed
                } else {
                    AssetStatus::Missing
                }
            }
            Err(_) => AssetStatus::Unknown,
        },
        Ok(response) if response.status() == StatusCode::NOT_FOUND => AssetStatus::Missing,
        Ok(_) | Err(_) => AssetStatus::Unknown,
    };

    (elasticsearch_assets, kibana_assets)
}

pub async fn assets_installed(elasticsearch: &Client, kibana: &Client) -> bool {
    let (elasticsearch_assets, kibana_assets) = asset_statuses(elasticsearch, kibana, None).await;
    elasticsearch_assets == AssetStatus::Installed && kibana_assets == AssetStatus::Installed
}

fn kibana_agent_path(kibana_url: Option<&str>) -> String {
    let url = kibana_url
        .map(str::to_string)
        .or_else(|| std::env::var("ESDIAG_KIBANA_URL").ok());
    url.and_then(|url| {
        Url::parse(&url).ok().and_then(|url| {
            let segments = url.path_segments()?.collect::<Vec<_>>();
            segments
                .windows(2)
                .find(|segments| segments[0] == "s")
                .map(|segments| format!("s/{}/api/agent_builder/agents/elastic-ai-agent", segments[1]))
        })
    })
    .unwrap_or_else(|| "/api/agent_builder/agents/elastic-ai-agent".to_string())
}

/// Start and verify a trial license before loading Enterprise-only Kibana assets.
pub async fn ensure_agent_builder_license(client: &Client) -> Result<()> {
    let Client::Elasticsearch(_) = client else {
        return Err(eyre!("an Elasticsearch client is required to start the trial license"));
    };

    if client.is_serverless().await? {
        tracing::info!("Serverless manages feature entitlements; skipping the Elasticsearch trial license API");
        return Ok(());
    }

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
    let mut bundle = kibana_bundle(embedded_assets)?.read_all()?;
    target_kibana_bundle(&mut bundle, crate::env::get_kibana_space().as_deref())?;
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

fn target_kibana_bundle(bundle: &mut SyncBundle, space: Option<&str>) -> Result<()> {
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

fn rewrite_kibana_asset_links(value: &mut Value, prefix: &str) {
    match value {
        Value::String(text) => *text = text.replace("/s/esdiag/", &format!("{prefix}/")),
        Value::Array(values) => values.iter_mut().for_each(|v| rewrite_kibana_asset_links(v, prefix)),
        Value::Object(values) => values.values_mut().for_each(|v| rewrite_kibana_asset_links(v, prefix)),
        _ => {}
    }
}

fn adapt_serverless_kibana_bundle(bundle: &mut SyncBundle) {
    for space in &mut bundle.spaces {
        if let Some(space) = space.as_object_mut() {
            // Serverless fixes the solution and disallows space feature visibility controls.
            space.remove("solution");
            space.remove("disabledFeatures");
        }
    }
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

fn default_agent_path(space_id: &str) -> String {
    let endpoint = "api/agent_builder/agents/elastic-ai-agent";
    if space_id == "default" {
        endpoint.to_string()
    } else {
        format!("s/{}/{endpoint}", urlencoding::encode(space_id))
    }
}

fn default_agent_skill_update(mut agent: Value, skill_ids: &[String]) -> Result<Value> {
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
fn parse_assets_yml(application: Application, assets_store: &EmbeddedAssets) -> Result<Vec<Asset>> {
    let filename = format!("{}/{}", application.key(), ASSETS_FILE);
    let contents = assets_store
        .get_file(Path::new(&filename))
        .ok_or(eyre!("embedded assets did not contain expected file {filename}"))?;
    let assets = yaml_serde::from_slice(&contents)?;
    Ok(assets)
}

#[cfg(test)]
#[path = "setup/serverless_tests.rs"]
mod serverless_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// The `diagnostic` properties an index created from the current templates
    /// carries, read from the template itself so the test moves with it.
    fn template_diagnostic_properties() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/elasticsearch/component_templates/esdiag@metadata.json");
        let template: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read metadata template")).expect("parse");
        template["template"]["mappings"]["properties"]["diagnostic"]["properties"].clone()
    }

    /// Which concrete field a provenance name resolves to in one index's mapping.
    fn resolves_to(diagnostic_properties: &Value, field: &str) -> Option<String> {
        let mapping = diagnostic_properties.get(field)?;
        match mapping["type"].as_str()? {
            "alias" => Some(mapping["path"].as_str().expect("alias path").to_string()),
            _ => Some(format!("diagnostic.{field}")),
        }
    }

    /// The mapping of an index created before the provenance rename.
    fn pre_rename_diagnostic_properties() -> Value {
        serde_json::json!({
            "product": { "type": "keyword" },
            "orchestration": { "type": "keyword" },
            "uuid": { "type": "keyword" },
        })
    }

    /// Applies a `_mapping` patch the way Elasticsearch would, so the assertion is
    /// about the mapping the index ends up with rather than the request body.
    fn patched(diagnostic_properties: &Value, patch: &Value) -> Value {
        let mut properties = diagnostic_properties.clone();
        let target = properties.as_object_mut().expect("diagnostic properties");
        let added = patch["properties"]["diagnostic"]["properties"]
            .as_object()
            .expect("patch properties");
        for (field, mapping) in added {
            target.insert(field.clone(), mapping.clone());
        }
        properties
    }

    #[test]
    fn both_provenance_names_resolve_in_either_index_generation() {
        let new_index = template_diagnostic_properties();
        for (current, legacy) in PROVENANCE_RENAMES {
            assert_eq!(new_index[current]["type"], "keyword");
            assert_eq!(new_index[legacy]["type"], "keyword");
        }
        assert!(provenance_mapping_warnings(&new_index).is_empty());
        assert!(
            provenance_alias_patch(&new_index).is_none(),
            "a new index needs no patch, so setup is idempotent"
        );

        let old_index = pre_rename_diagnostic_properties();
        assert_eq!(
            resolves_to(&old_index, "application"),
            None,
            "before the patch, the current name resolves to nothing in a historical index"
        );

        let patch = provenance_alias_patch(&old_index).expect("a pre-rename index needs the mirrored aliases");
        let old_index = patched(&old_index, &patch);
        assert_eq!(
            resolves_to(&old_index, "application"),
            resolves_to(&old_index, "product"),
            "after the patch, an old index stores `product` and aliases `application` to it"
        );
        assert_eq!(
            resolves_to(&old_index, "orchestration"),
            resolves_to(&old_index, "platform")
        );
        assert!(
            provenance_alias_patch(&old_index).is_none(),
            "re-running setup over a patched index is a no-op"
        );
    }

    #[test]
    fn provenance_warnings_identify_aliases_and_unlinked_fields() {
        let old = pre_rename_diagnostic_properties();
        assert!(provenance_mapping_warnings(&old).is_empty());
        let patch = provenance_alias_patch(&old).unwrap();
        assert_eq!(provenance_mapping_warnings(&patched(&old, &patch)).len(), 2);
        let split = serde_json::json!({
            "application": {"type": "keyword"},
            "product": {"type": "keyword"}
        });
        assert!(provenance_mapping_warnings(&split)[0].contains("unlinked concrete fields"));
        assert!(provenance_alias_patch(&split).is_none());
        let alias = serde_json::json!({
            "application": {"type": "keyword"},
            "product": {"type": "alias", "path": "diagnostic.application"}
        });
        assert!(provenance_mapping_warnings(&alias)[0].contains("diagnostic.product is a query-only alias"));
        assert!(provenance_mapping_warnings(&Value::Null).is_empty());
    }

    #[test]
    fn provenance_alias_patch_skips_mappings_with_nothing_to_bridge() {
        assert!(provenance_alias_patch(&Value::Null).is_none());
        assert!(provenance_alias_patch(&serde_json::json!({ "uuid": { "type": "keyword" } })).is_none());
        assert!(
            provenance_alias_patch(&serde_json::json!({
                "product": { "type": "alias", "path": "diagnostic.application" },
                "application": { "type": "keyword" },
            }))
            .is_none()
        );
    }

    #[test]
    fn provenance_alias_patches_select_only_the_pre_rename_indices() {
        let mappings = serde_json::json!({
            ".ds-metrics-node-esdiag-2024.01.01-000001": {
                "mappings": { "properties": { "diagnostic": { "properties": pre_rename_diagnostic_properties() } } }
            },
            ".ds-metrics-node-esdiag-2026.07.01-000002": {
                "mappings": { "properties": { "diagnostic": { "properties": template_diagnostic_properties() } } }
            },
            "unrelated-esdiag-lookalike": { "mappings": { "properties": { "message": { "type": "text" } } } },
        });

        let patches = provenance_alias_patches(&mappings);

        assert_eq!(patches.len(), 1, "only the historical backing index is patched");
        let (index, patch) = &patches[0];
        assert_eq!(index, ".ds-metrics-node-esdiag-2024.01.01-000001");
        assert_eq!(
            patch["properties"]["diagnostic"]["properties"]["application"],
            serde_json::json!({ "type": "alias", "path": "diagnostic.product" })
        );
    }

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
        let assets: Vec<Asset> = yaml_serde::from_str(yaml).unwrap();
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
        assert!(!KIBANA_ASSETS_BUNDLE.is_empty());
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
            let label = saved_object_label(object);
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
