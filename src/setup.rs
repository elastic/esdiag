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
}

fn default_headers() -> HashMap<String, String> {
    HashMap::from([("Content-Type".to_string(), "application/json".to_string())])
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

/// Submit saved assets to the Elasticsearch APIs
pub async fn assets(client: &Client) -> Result<()> {
    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into())?;

    for asset in assets {
        log::info!("Processing asset: {}", &asset.name);
        log::debug!("Asset: {}", serde_json::to_string(&asset).unwrap());
        let path = PathBuf::from(format!("{}/{}", client, asset.name));
        if let Some(dir) = ASSETS_DIR.get_dir(&path) {
            // do something with the directory
            for file in dir.files() {
                log::debug!("file.path: {:?}", &file.path());
                match send_asset(client, &asset, file, true).await {
                    Ok(res) => log::debug!("Response: {:?}", res),
                    Err(e) => log::error!("Failed to send asset: {e:?}"),
                }
            }
        } else if let Some(file) = ASSETS_DIR.get_file(&path) {
            // do something with the file
            log::debug!("file.path: {:?}", &file.path());
            if let Err(e) = send_asset(client, &asset, file, false).await {
                log::error!("Failed to send asset: {e:?}");
            }
        } else {
            log::error!("Asset not found: {}", &asset.name);
            return Err(eyre!("Asset not found: {}", asset.name));
        }
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
