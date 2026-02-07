// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{client::Client, data::Product};
//use bytes::Bytes;
use eyre::{Result, eyre};
use include_dir::{Dir, File, include_dir};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

// Subdirectory for templates and configs files
pub static ASSETS_DIR: Dir = include_dir!("assets");
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";

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

async fn send_asset(client: &Client, asset: &Asset, file: &File<'_>, named: bool) -> Result<()> {
    let stem = file.path().file_stem().unwrap().to_str().unwrap_or("");
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
            Some(file.contents()),
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
                    let body = response.json::<Value>().await?;
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
    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into())?;

    // Check security status
    let security_enabled = client.has_security_enabled().await.unwrap_or(false);
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
        if let Some(dir) = ASSETS_DIR.get_dir(&path) {
            // do something with the directory
            for file in dir.files() {
                log::debug!("file.path: {:?}", &file.path());
                match send_asset(client, &asset, file, true).await {
                    Ok(res) => log::debug!("Response: {:?}", res),
                    Err(e) => {
                        log::error!("Failed to send asset: {e:?}");
                        error_count += 1;
                    }
                }
            }
        } else if let Some(file) = ASSETS_DIR.get_file(&path) {
            // do something with the file
            log::debug!("file.path: {:?}", &file.path());
            if let Err(e) = send_asset(client, &asset, file, false).await {
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
fn parse_assets_yml(product: Product) -> Result<Vec<Asset>> {
    let filename = format!("{}/{}", product.to_string().to_lowercase(), ASSETS_FILE);
    let file = ASSETS_DIR
        .get_file(&filename)
        .ok_or(eyre!("Error reading {filename}"))?;
    let assets = serde_yaml::from_slice(file.contents())?;
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
