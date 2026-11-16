// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::*;
use crate::client::Client;
//use bytes::Bytes;
use eyre::{Result, WrapErr, eyre};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

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

pub(super) fn default_headers() -> HashMap<String, String> {
    HashMap::from([("Content-Type".to_string(), "application/json".to_string())])
}

/// Every ESDiag-owned index, whichever generation created it.
pub(super) const ESDIAG_INDEX_PATTERN: &str = "*-esdiag*";

/// The renamed `diagnostic.*` provenance fields as `(current, legacy)` pairs
/// (ADR-0001, bridged by ADR-0014).
pub(super) const PROVENANCE_RENAMES: [(&str, &str); 2] = [("application", "product"), ("platform", "orchestration")];

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
pub(super) fn provenance_alias_patch(diagnostic_properties: &Value) -> Option<Value> {
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
pub(super) fn provenance_alias_patches(mappings: &Value) -> Vec<(String, Value)> {
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

/// A provenance mapping that installing a new template cannot repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ProvenanceIssue {
    /// Aliases cannot accept values, so writers using this name are rejected.
    QueryOnlyAlias(&'static str),
    /// Both names are stored without reciprocal `copy_to`, so a search by one
    /// name misses documents stored under the other.
    Unlinked {
        current: &'static str,
        legacy: &'static str,
    },
}

impl std::fmt::Display for ProvenanceIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueryOnlyAlias(name) => write!(
                f,
                "diagnostic.{name} is a query-only alias and rejects writers using that name; \
                 roll over the data stream after setup before mixing writer versions"
            ),
            Self::Unlinked { current, legacy } => write!(
                f,
                "diagnostic.{current} and diagnostic.{legacy} are unlinked concrete fields; \
                 historical search results may be split. Roll over for future writes and \
                 reindex historical documents if both names must find them"
            ),
        }
    }
}

pub(super) fn provenance_mapping_issues(properties: &Value) -> Vec<ProvenanceIssue> {
    let mut issues = Vec::new();
    for (current, legacy) in PROVENANCE_RENAMES {
        for name in [current, legacy] {
            if properties[name]["type"] == "alias" {
                issues.push(ProvenanceIssue::QueryOnlyAlias(name));
            }
        }
        let concrete = |name: &str| properties[name]["type"].as_str().is_some_and(|kind| kind != "alias");
        let copies_to = |from: &str, to: &str| {
            let target = Value::String(format!("diagnostic.{to}"));
            let copy = &properties[from]["copy_to"];
            copy == &target || copy.as_array().is_some_and(|targets| targets.contains(&target))
        };
        if concrete(current) && concrete(legacy) && !(copies_to(current, legacy) && copies_to(legacy, current)) {
            issues.push(ProvenanceIssue::Unlinked { current, legacy });
        }
    }
    issues
}

/// Backing indices behind each data stream's write index, mapped to their data
/// stream, from a `GET _data_stream` response. Writers never reach these again.
pub(super) fn historical_backing_indices(data_streams: &Value) -> HashMap<String, String> {
    let mut historical = HashMap::new();
    for stream in data_streams["data_streams"].as_array().into_iter().flatten() {
        let Some(name) = stream["name"].as_str() else {
            continue;
        };
        let Some((_write_index, older)) = stream["indices"].as_array().and_then(|indices| indices.split_last()) else {
            continue;
        };
        for index in older.iter().filter_map(|index| index["index_name"].as_str()) {
            historical.insert(index.to_string(), name.to_string());
        }
    }
    historical
}

/// Setup warnings for the provenance mappings in a `GET <pattern>/_mapping`
/// response.
///
/// Indices that can still receive writes are reported one by one. Historical
/// backing indices only matter for searches: their aliases never see a writer,
/// and their unlinked fields are summarized once per data stream, because only a
/// reindex changes them and repeating them on every setup buries real failures.
pub(super) fn provenance_mapping_warnings(mappings: &Value, historical: &HashMap<String, String>) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut unlinked_by_stream: BTreeMap<&str, (usize, BTreeSet<ProvenanceIssue>)> = BTreeMap::new();
    let mut indices: Vec<_> = mappings.as_object().into_iter().flatten().collect();
    indices.sort_by_key(|(index, _)| index.as_str());
    for (index, mapping) in indices {
        let properties = mapping
            .pointer("/mappings/properties/diagnostic/properties")
            .unwrap_or(&Value::Null);
        let issues = provenance_mapping_issues(properties);
        match historical.get(index.as_str()) {
            None => warnings.extend(issues.iter().map(|issue| format!("{index}: {issue}"))),
            Some(stream) => {
                let (unlinked, aliases): (Vec<_>, Vec<_>) = issues
                    .into_iter()
                    .partition(|issue| matches!(issue, ProvenanceIssue::Unlinked { .. }));
                for alias in aliases {
                    tracing::debug!("{index}: {alias}");
                }
                if !unlinked.is_empty() {
                    let (count, pairs) = unlinked_by_stream.entry(stream.as_str()).or_default();
                    *count += 1;
                    pairs.extend(unlinked);
                }
            }
        }
    }
    for (stream, (count, pairs)) in unlinked_by_stream {
        let pairs = pairs
            .iter()
            .filter_map(|issue| match issue {
                ProvenanceIssue::Unlinked { current, legacy } => {
                    Some(format!("diagnostic.{current} and diagnostic.{legacy}"))
                }
                ProvenanceIssue::QueryOnlyAlias(_) => None,
            })
            .collect::<Vec<_>>()
            .join("; ");
        let indices = if count == 1 { "index stores" } else { "indices store" };
        warnings.push(format!(
            "{stream}: {count} older backing {indices} {pairs} as unlinked concrete fields, so searches by \
             one name miss documents stored under the other. Reindex them if both names must find those documents"
        ));
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
pub(super) async fn install_provenance_aliases(client: &Client) -> Result<Vec<String>> {
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
    let historical = read_historical_backing_indices(client).await;
    for warning in provenance_mapping_warnings(&mappings, &historical) {
        tracing::warn!("{warning}");
    }
    let patches = provenance_alias_patches(&mappings);
    if patches.is_empty() {
        tracing::debug!("No pre-rename ESDiag indices need a provenance alias");
        return Ok(vec![]);
    }

    let mut failures = Vec::new();
    let mut aliased_writable_indices = false;
    for (index, patch) in &patches {
        let body = serde_json::to_vec(patch)?;
        let result = client
            .request(Method::PUT, &headers, &format!("/{index}/_mapping"), Some(&body))
            .await;
        match result {
            Ok(response) if response.status().is_success() => {
                tracing::debug!("Aliased the current provenance names onto {index}");
                aliased_writable_indices |= !historical.contains_key(index);
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
    if aliased_writable_indices {
        tracing::warn!(
            "Added query-only provenance aliases. Roll over the affected data streams after setup before mixing writer versions."
        );
    }
    Ok(failures)
}

/// Without the data stream listing every index is treated as writable, which
/// only makes the provenance warnings more verbose.
pub(super) async fn read_historical_backing_indices(client: &Client) -> HashMap<String, String> {
    let path = format!("/_data_stream/{ESDIAG_INDEX_PATTERN}");
    let response = match client.request(Method::GET, &default_headers(), &path, None).await {
        Ok(response) => response,
        Err(err) => {
            tracing::debug!("Could not list ESDiag data streams: {err}");
            return HashMap::new();
        }
    };
    if !response.status().is_success() {
        tracing::debug!("Listing ESDiag data streams returned {}", response.status());
        return HashMap::new();
    }
    match response.json::<Value>().await {
        Ok(data_streams) => historical_backing_indices(&data_streams),
        Err(err) => {
            tracing::debug!("Could not read the ESDiag data stream listing: {err}");
            HashMap::new()
        }
    }
}

pub(super) fn should_skip_asset(asset: &Asset, security_enabled: bool) -> bool {
    asset.requires_security && !security_enabled
}

#[derive(Debug)]
pub(super) struct AssetRejected {
    status: StatusCode,
    body: Value,
}

impl std::fmt::Display for AssetRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Asset: {}", self.body)
    }
}

impl std::error::Error for AssetRejected {}

/// Serverless setup credentials often lack `manage_security`; the rest of
/// setup must still complete so onboarding can proceed.
pub(super) fn is_forbidden_security_asset(asset: &Asset, error: &eyre::Report) -> bool {
    asset.requires_security
        && error
            .downcast_ref::<AssetRejected>()
            .is_some_and(|rejected| rejected.status == StatusCode::FORBIDDEN)
}

pub(super) fn warn_forbidden_security_asset(path: &Path) {
    let name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
    tracing::warn!(
        "Setup credentials cannot manage roles in this Serverless project. Ask an administrator to create the {name} role from the bundled definition so users can read failure stores."
    );
}

pub(super) async fn send_asset(
    client: &Client,
    asset: &Asset,
    path: &Path,
    contents: &[u8],
    named: bool,
) -> Result<()> {
    send_asset_with_allowed_statuses(client, asset, path, contents, named, &[]).await
}

/// Adapt only template settings. ILM fields in diagnostic mappings are source data.
pub(super) fn serverless_asset_contents(asset: &Asset, contents: &[u8]) -> Result<Vec<u8>> {
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

/// The `version` of the installed template, if it exists and has one.
pub(super) async fn installed_template_version(client: &Client, asset: &Asset, name: &str) -> Option<u64> {
    let path = format!("{}/{name}", asset.endpoint);
    let response = client
        .request(Method::GET, &default_headers(), &path, None)
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    template_version(&body, asset.endpoint.trim_matches('/'), name)
}

pub(super) fn template_version(body: &Value, endpoint: &str, name: &str) -> Option<u64> {
    let (list, item) = match endpoint {
        "_index_template" => ("index_templates", "index_template"),
        _ => ("component_templates", "component_template"),
    };
    body.get(list)?
        .as_array()?
        .iter()
        .find(|template| template["name"] == name)?
        .pointer(&format!("/{item}/version"))?
        .as_u64()
}

/// Bundled templates are versioned by hand, and the version is only bumped when
/// existing data streams need a rollover to pick up the change.
pub(super) fn template_version_bumped(installed: Option<u64>, bundled: Option<u64>) -> bool {
    match (installed, bundled) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(installed), Some(bundled)) => installed < bundled,
    }
}

/// A newer ESDiag release installed this template; an older binary must not
/// overwrite it, because its streams would not roll over back to the old mappings.
pub(super) fn template_version_downgrade(installed: Option<u64>, bundled: Option<u64>) -> bool {
    match (installed, bundled) {
        (Some(installed), Some(bundled)) => installed > bundled,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Templates installed by this setup run with a higher version than was installed before.
#[derive(Default)]
pub(super) struct TemplateChanges {
    component_templates: HashSet<String>,
    index_templates: HashSet<String>,
    composed_of: HashMap<String, Vec<String>>,
}

impl TemplateChanges {
    pub(super) fn record(&mut self, asset: &Asset, name: &str, contents: &[u8], changed: bool) {
        let index_template = asset.endpoint.trim_matches('/') == "_index_template";
        if index_template && let Ok(body) = serde_json::from_slice::<Value>(contents) {
            let components = body["composed_of"]
                .as_array()
                .map(|names| names.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            self.composed_of.insert(name.to_string(), components);
        }
        if !changed {
            return;
        }
        match index_template {
            true => self.index_templates.insert(name.to_string()),
            false => self.component_templates.insert(name.to_string()),
        };
    }

    /// Index templates that changed directly or through a component template.
    pub(super) fn updated_index_templates(&self) -> HashSet<String> {
        let mut updated = self.index_templates.clone();
        for (template, components) in &self.composed_of {
            if components.iter().any(|name| self.component_templates.contains(name)) {
                updated.insert(template.clone());
            }
        }
        updated
    }
}

/// `(data stream, index template)` pairs from a `GET _data_stream` response for
/// streams that currently resolve to one of the updated templates.
pub(super) fn data_streams_to_roll_over(
    data_streams: &Value,
    updated_templates: &HashSet<String>,
) -> Vec<(String, String)> {
    let mut streams: Vec<_> = data_streams["data_streams"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|stream| {
            let name = stream["name"].as_str()?;
            let template = stream["template"].as_str()?;
            updated_templates
                .contains(template)
                .then(|| (name.to_string(), template.to_string()))
        })
        .collect();
    streams.sort();
    streams
}

/// Templates only shape backing indices created after they are installed, so
/// existing data streams keep their old mappings and settings until rollover.
/// Returns the streams that could not be rolled over.
pub(super) async fn rollover_updated_data_streams(
    client: &Client,
    updated_templates: &HashSet<String>,
) -> Result<Vec<String>> {
    let headers = default_headers();
    let path = format!("/_data_stream/{ESDIAG_INDEX_PATTERN}");
    let response = client.request(Method::GET, &headers, &path, None).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    if !response.status().is_success() {
        return Err(eyre!(
            "Listing ESDiag data streams returned {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let data_streams: Value = response.json().await?;
    let streams = data_streams_to_roll_over(&data_streams, updated_templates);

    let mut failures = Vec::new();
    for (stream, template) in &streams {
        let result = client
            .request(Method::POST, &headers, &format!("/{stream}/_rollover"), None)
            .await;
        match result {
            Ok(response) if response.status().is_success() => {
                tracing::info!("Rolled over data stream {stream} to apply the updated {template} index template");
            }
            Ok(response) => {
                failures.push(stream.clone());
                tracing::warn!(
                    "Could not roll over data stream {stream}: {} {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
            }
            Err(err) => {
                failures.push(stream.clone());
                tracing::warn!("Could not roll over data stream {stream}: {err}");
            }
        }
    }
    if !streams.is_empty() {
        tracing::info!(
            "Rolled over {} of {} data streams with updated templates",
            streams.len() - failures.len(),
            streams.len()
        );
    }
    Ok(failures)
}

pub(super) fn is_template_asset(asset: &Asset) -> bool {
    matches!(
        asset.endpoint.trim_matches('/'),
        "_component_template" | "_index_template"
    )
}

pub(super) fn remove_serverless_ilm_settings(value: &mut Value, prefix: &str) {
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

pub(super) async fn send_asset_with_allowed_statuses(
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
                    Err(AssetRejected { status, body }.into())
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to send asset: {e:?}");
            Err(eyre!(e))
        }
    }
}

pub(super) async fn api_assets(client: &Client, embedded_assets: &EmbeddedAssets) -> Result<SetupReport> {
    let mut report = SetupReport::default();
    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into(), embedded_assets)?;

    // Check security status
    let serverless = client.is_serverless().await?;
    let security_enabled = serverless
        || client
            .has_security_enabled()
            .await
            .wrap_err("Failed to determine security status")?;
    if !security_enabled {
        tracing::info!("Security is disabled on the cluster. Security-dependent assets will be skipped.");
    }

    let mut error_count = 0;
    let mut template_changes = TemplateChanges::default();

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
                let contents = if serverless && is_template_asset(&asset) {
                    std::borrow::Cow::Owned(serverless_asset_contents(&asset, &contents)?)
                } else {
                    contents
                };
                tracing::debug!("file.path: {:?}", file_path);
                let template = match is_template_asset(&asset) {
                    true => {
                        let stem = file_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
                        let name = format!("{stem}{}", asset.suffix.as_deref().unwrap_or(""));
                        let installed = installed_template_version(client, &asset, &name).await;
                        let bundled = serde_json::from_slice::<Value>(&contents)
                            .ok()
                            .and_then(|body| body["version"].as_u64());
                        if template_version_downgrade(installed, bundled) {
                            tracing::warn!(
                                "Keeping installed template {name} (version {}), which is newer than this ESDiag release's bundled version. Upgrade ESDiag to update it.",
                                installed.unwrap_or_default()
                            );
                            continue;
                        }
                        Some((name, template_version_bumped(installed, bundled)))
                    }
                    false => None,
                };
                match send_asset(client, &asset, &file_path, &contents, true).await {
                    Ok(res) => {
                        tracing::debug!("Response: {:?}", res);
                        if let Some((name, changed)) = &template {
                            template_changes.record(&asset, name, &contents, *changed);
                        }
                    }
                    Err(e) if serverless && is_forbidden_security_asset(&asset, &e) => {
                        warn_forbidden_security_asset(&file_path);
                    }
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
            match send_asset(client, &asset, &path, &contents, false).await {
                Err(e) if serverless && is_forbidden_security_asset(&asset, &e) => warn_forbidden_security_asset(&path),
                Err(e) => {
                    tracing::error!("Failed to send asset: {e:?}");
                    report.warnings.push(format!(
                        "Failed to install {} asset {}. Check setup logs and rerun setup.",
                        client,
                        path.display()
                    ));
                    error_count += 1;
                }
                Ok(()) => {}
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
        let updated_templates = template_changes.updated_index_templates();
        if !updated_templates.is_empty() {
            match rollover_updated_data_streams(client, &updated_templates).await {
                Ok(failed) if failed.is_empty() => {}
                Ok(failed) => report.warnings.push(format!(
                    "Could not roll over data streams {} to apply updated templates. Roll them over manually, then rerun failed diagnostics.",
                    failed.join(", ")
                )),
                Err(err) => {
                    tracing::warn!("Could not roll over data streams with updated templates: {err}");
                    report.warnings.push("Could not list ESDiag data streams to apply updated templates. Check connectivity and data stream privileges, then rerun setup.".to_string());
                }
            }
        }
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
