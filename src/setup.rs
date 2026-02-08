// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{client::Client, data::Product};
//use bytes::Bytes;
use eyre::{Result, eyre, WrapErr};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;

// Subdirectory for templates and configs files
pub static ASSETS_TAR_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/assets.tar.gz"));
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";

struct EmbeddedAssets {
    files: HashMap<PathBuf, Vec<u8>>,
}

impl EmbeddedAssets {
    fn new() -> Result<Self> {
        let mut files = HashMap::new();
        let tar_gz = GzDecoder::new(ASSETS_TAR_GZ);
        let mut archive = Archive::new(tar_gz);
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.header().entry_type().is_file() {
                let mut path = entry.path()?.to_path_buf();
                if let Ok(stripped) = path.strip_prefix("assets") {
                    path = stripped.to_path_buf();
                }
                let mut content = Vec::new();
                entry.read_to_end(&mut content)?;
                files.insert(path, content);
            }
        }
        Ok(Self { files })
    }

    fn get_file(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(|v| v.as_slice())
    }

    fn get_dir_files(&self, path: &Path) -> Vec<(&Path, &[u8])> {
        let mut files: Vec<_> = self.files
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, v)| (p.as_path(), v.as_slice()))
            .collect();
        files.sort_by_key(|(p, _)| *p);
        files
    }
}

#[derive(Deserialize, Serialize)]
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
        .request(
            asset.method.parse()?,
            &asset.headers,
            endpoint,
            Some(contents),
        )
        .await
    {
        Ok(response) => {
            let status = response.status();
            match status.is_success() {
                true => {
                    let body = response.text().await?;
                    log::info!("{} {} {} {}", &asset.name, &stem, &asset.method, status);
                    log::trace!("Response body: {}", body);
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
            log::error!("Failed to send asset: {e:?}");
            Err(eyre!(e))
        }
    }
}

/// Submit saved assets to the client APIs
pub async fn assets(client: &Client) -> Result<()> {
    let embedded_assets = EmbeddedAssets::new()?;
    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into(), &embedded_assets)?;

    // Check security status
    let security_enabled = client
        .has_security_enabled()
        .await
        .wrap_err("Failed to determine security status")?;

    if !security_enabled {
        log::info!("Security is disabled on the cluster. Security-dependent assets will be skipped.");
    }

    let mut error_count = 0;

    for asset in assets {
        if should_skip_asset(&asset, security_enabled) {
            log::debug!("Skipping security-dependent asset: {}", &asset.name);
            continue;
        }

        log::info!("Processing asset: {}", &asset.name);
        log::debug!("Asset: {}", serde_json::to_string(&asset).unwrap());
        let path = PathBuf::from(format!("{}/{}", client, asset.name));
        
        let dir_files = embedded_assets.get_dir_files(&path);
        if !dir_files.is_empty() {
            // do something with the directory
            for (file_path, contents) in dir_files {
                log::debug!("file.path: {:?}", file_path);
                match send_asset(client, &asset, file_path, contents, true).await {
                    Ok(res) => log::debug!("Response: {:?}", res),
                    Err(e) => {
                        log::error!("Failed to send asset: {e:?}");
                        error_count += 1;
                    }
                }
            }
        } else if let Some(contents) = embedded_assets.get_file(&path) {
            // do something with the file
            log::debug!("file.path: {:?}", &path);
            if let Err(e) = send_asset(client, &asset, &path, contents, false).await {
                log::error!("Failed to send asset: {e:?}");
                error_count += 1;
            }
        } else {
            log::error!("Asset not found: {}", &asset.name);
            return Err(eyre!("Asset not found: {}", asset.name));
        }
    }
    match error_count {
        0 => log::info!("completed setup for {client}"),
        _ => log::error!("{error_count} errors in setup for {client}"),
    }
    Ok(())
}

/// Parses the assets YAML file for the given exporter. Currently only supports Elasticsearch.
fn parse_assets_yml(product: Product, assets_store: &EmbeddedAssets) -> Result<Vec<Asset>> {
    let filename = format!("{}/{}", product.to_string().to_lowercase(), ASSETS_FILE);
    let contents = assets_store
        .get_file(Path::new(&filename))
        .ok_or(eyre!("embedded assets archive (assets.tar.gz) did not contain expected file {filename}"))?;
    let assets = serde_yaml::from_slice(contents)?;
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
}
