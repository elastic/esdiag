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
use kibana_sync::kibana::{
    saved_objects::{SavedObject, SavedObjectsManifest},
    spaces::{SpaceEntry, SpacesManifest},
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

// Subdirectory for templates and configs files
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";
const KIBANA_ASSETS_DIR: &str = "kibana";
const KIBANA_SPACES_FILE: &str = "spaces.yml";
const KIBANA_SPACE_DEFINITION_FILE: &str = "space.json";
const KIBANA_MANIFEST_DIR: &str = "manifest";
const KIBANA_OBJECTS_DIR: &str = "objects";
const KIBANA_SAVED_OBJECTS_MANIFEST: &str = "saved_objects.json";

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
            match status.is_success() {
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
    match error_count {
        0 => tracing::info!("completed setup for {client}"),
        _ => tracing::error!("{error_count} errors in setup for {client}"),
    }
    Ok(())
}

async fn kibana_assets(client: &Client, embedded_assets: &EmbeddedAssets) -> Result<()> {
    let spaces_manifest = parse_kibana_spaces_yml(embedded_assets)?;
    let mut error_count = 0;

    for space in &spaces_manifest.spaces {
        let space_payload = kibana_space_payload(space, embedded_assets)?;
        let space_asset = Asset {
            endpoint: "api/spaces".to_string(),
            method: Method::POST.to_string(),
            name: KIBANA_SPACE_DEFINITION_FILE.to_string(),
            headers: default_headers(),
            suffix: None,
            query: None,
            requires_security: false,
        };
        let space_path = kibana_space_definition_path(&space.id);
        if let Err(e) = send_asset(client, &space_asset, &space_path, &space_payload, false).await {
            tracing::error!("Failed to send Kibana space asset: {e:?}");
            error_count += 1;
        }

        let saved_objects = kibana_saved_objects_ndjson(&space.id, embedded_assets)?;
        if saved_objects.is_empty() {
            continue;
        }

        let saved_objects_asset = Asset {
            endpoint: format!("s/{}/api/saved_objects/_import?overwrite", space.id),
            method: Method::POST.to_string(),
            name: KIBANA_SAVED_OBJECTS_MANIFEST.to_string(),
            headers: HashMap::from([("Content-Type".to_string(), "multipart/form-data".to_string())]),
            suffix: None,
            query: None,
            requires_security: false,
        };
        let saved_objects_path = kibana_saved_objects_manifest_path(&space.id);
        if let Err(e) = send_asset(client, &saved_objects_asset, &saved_objects_path, &saved_objects, false).await {
            tracing::error!("Failed to send Kibana saved objects asset: {e:?}");
            error_count += 1;
        }
    }

    match error_count {
        0 => tracing::info!("completed setup for {client}"),
        _ => tracing::error!("{error_count} errors in setup for {client}"),
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

fn parse_kibana_spaces_yml(assets_store: &EmbeddedAssets) -> Result<SpacesManifest> {
    let filename = PathBuf::from(KIBANA_ASSETS_DIR).join(KIBANA_SPACES_FILE);
    let contents = assets_store.get_file(&filename).ok_or(eyre!(
        "embedded assets did not contain expected file {}",
        filename.display()
    ))?;
    let manifest = serde_yaml::from_slice(&contents)?;
    Ok(manifest)
}

fn parse_kibana_saved_objects_manifest(space_id: &str, assets_store: &EmbeddedAssets) -> Result<SavedObjectsManifest> {
    let filename = kibana_saved_objects_manifest_path(space_id);
    let contents = assets_store.get_file(&filename).ok_or(eyre!(
        "embedded assets did not contain expected file {}",
        filename.display()
    ))?;
    let manifest = serde_json::from_slice(&contents)?;
    Ok(manifest)
}

fn kibana_space_payload(space: &SpaceEntry, assets_store: &EmbeddedAssets) -> Result<Vec<u8>> {
    let path = kibana_space_definition_path(&space.id);
    if let Some(contents) = assets_store.get_file(&path) {
        return Ok(contents.into_owned());
    }

    serde_json::to_vec(&json!({
        "id": space.id,
        "name": space.name,
    }))
    .map_err(Into::into)
}

fn kibana_saved_objects_ndjson(space_id: &str, assets_store: &EmbeddedAssets) -> Result<Vec<u8>> {
    let manifest = parse_kibana_saved_objects_manifest(space_id, assets_store)?;
    let mut out = Vec::new();

    for object in manifest.objects {
        let path = kibana_saved_object_path(space_id, &object);
        let contents = assets_store.get_file(&path).ok_or(eyre!(
            "embedded assets did not contain expected file {}",
            path.display()
        ))?;
        let value: Value = serde_json::from_slice(&contents)
            .wrap_err_with(|| format!("Failed to parse Kibana saved object {}", path.display()))?;
        ensure_saved_object_matches_manifest(&value, &object, &path)?;
        serde_json::to_writer(&mut out, &value)?;
        out.push(b'\n');
    }

    Ok(out)
}

fn ensure_saved_object_matches_manifest(value: &Value, object: &SavedObject, path: &Path) -> Result<()> {
    let actual_type = value.get("type").and_then(Value::as_str);
    let actual_id = value.get("id").and_then(Value::as_str);
    if actual_type != Some(object.object_type.as_str()) || actual_id != Some(object.id.as_str()) {
        return Err(eyre!(
            "saved object {} does not match manifest entry {}/{}",
            path.display(),
            object.object_type,
            object.id
        ));
    }
    Ok(())
}

fn kibana_space_definition_path(space_id: &str) -> PathBuf {
    PathBuf::from(KIBANA_ASSETS_DIR)
        .join(space_id)
        .join(KIBANA_SPACE_DEFINITION_FILE)
}

fn kibana_saved_objects_manifest_path(space_id: &str) -> PathBuf {
    PathBuf::from(KIBANA_ASSETS_DIR)
        .join(space_id)
        .join(KIBANA_MANIFEST_DIR)
        .join(KIBANA_SAVED_OBJECTS_MANIFEST)
}

fn kibana_saved_object_path(space_id: &str, object: &SavedObject) -> PathBuf {
    PathBuf::from(KIBANA_ASSETS_DIR)
        .join(space_id)
        .join(KIBANA_OBJECTS_DIR)
        .join(sanitize_kibana_asset_component(&object.object_type))
        .join(format!("{}.json", sanitize_kibana_asset_component(&object.id)))
}

fn sanitize_kibana_asset_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '&' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        "unnamed".to_string()
    } else {
        sanitized
    }
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
        let bundle = kibana_sync::KibanaFsBundle::open("assets/kibana")
            .unwrap()
            .read_all()
            .unwrap();

        assert_eq!(bundle.spaces.len(), 1);
        assert_eq!(bundle.spaces[0]["id"], "esdiag");

        let esdiag = bundle.by_space.get("esdiag").unwrap();
        assert_eq!(esdiag.saved_objects.len(), 90);
        assert!(esdiag.workflows.is_empty());
        assert!(esdiag.agents.is_empty());
        assert!(esdiag.tools.is_empty());
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
    fn kibana_space_payload_preserves_full_space_definition() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let spaces = parse_kibana_spaces_yml(&embedded_assets).unwrap();
        let payload = kibana_space_payload(&spaces.spaces[0], &embedded_assets).unwrap();
        let value: Value = serde_json::from_slice(&payload).unwrap();

        assert_eq!(value["id"], "esdiag");
        assert_eq!(value["description"], "Elastic Stack Diagnostics");
        assert_eq!(value["solution"], "classic");
        assert!(value["disabledFeatures"].as_array().unwrap().contains(&json!("canvas")));
    }

    #[test]
    fn kibana_saved_objects_ndjson_uses_manifest_order() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let manifest = parse_kibana_saved_objects_manifest("esdiag", &embedded_assets).unwrap();
        let ndjson = kibana_saved_objects_ndjson("esdiag", &embedded_assets).unwrap();
        let lines: Vec<_> = ndjson
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect();

        assert_eq!(lines.len(), manifest.objects.len());
        assert_eq!(manifest.objects.len(), 90);

        let first: Value = serde_json::from_slice(lines[0]).unwrap();
        assert_eq!(first["type"], manifest.objects[0].object_type);
        assert_eq!(first["id"], manifest.objects[0].id);
    }

    #[test]
    fn kibana_saved_objects_have_valid_embedded_json_content() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let ndjson = kibana_saved_objects_ndjson("esdiag", &embedded_assets).unwrap();

        for line in ndjson.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
            let object: Value = serde_json::from_slice(line).unwrap();
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
        let path = Path::new("kibana/esdiag/objects/dashboard/esdiag-readme.json");
        let contents = embedded_assets
            .get_file(path)
            .expect("readme dashboard should be embedded");
        let object: Value = serde_json::from_slice(&contents).unwrap();
        let panels_json = object["attributes"]["panelsJSON"].as_str().unwrap();

        assert!(panels_json.contains("https://github.com/elastic/esdiag/issues"));
        assert!(!panels_json.contains("https://github.com/elastic/issues)"));
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
