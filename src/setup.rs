// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{client::Client, data::Product};
//use bytes::Bytes;
use eyre::{Result, eyre};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

// Subdirectory for templates and configs files
pub static ASSETS_DIR: Dir = include_dir!("assets");
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";

#[derive(Deserialize)]
pub struct Asset {
    pub endpoint: String,
    pub method: String,
    pub name: String,
    pub subdir: Option<String>,
    pub suffix: Option<String>,
}

/// Submit saved assets to the Elasticsearch APIs
pub async fn assets(client: &Client) -> Result<()> {
    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(client.into())?;

    for asset in assets {
        log::info!("Processing asset: {}", &asset.name);
        let dir_str = format!("{}/{}", client, &asset.subdir.unwrap_or("".to_string()));
        let subdir = PathBuf::from(dir_str);
        let files = match ASSETS_DIR.get_dir(&subdir) {
            Some(dir) => dir.files(),
            None => return Err(eyre!("No assets directory found")),
        };

        match client {
            Client::Elasticsearch(_) => {
                // for each asset, send to Elasticsearch
                for file in files {
                    log::debug!("file.path: {:?}", &file.path());
                    let stem = file.path().file_stem().unwrap().to_str().unwrap_or("");
                    let endpoint = format!(
                        "{}/{}{}",
                        &asset.endpoint,
                        &stem,
                        asset.suffix.clone().unwrap_or("".to_string()),
                    );
                    match client
                        .request(asset.method.parse()?, &endpoint, Some(file.contents()))
                        .await
                    {
                        Ok(response) => match response.status().is_success() {
                            true => {
                                log::info!(
                                    "{} {} {} {}",
                                    &asset.name,
                                    &stem,
                                    &asset.method,
                                    response.status()
                                )
                            }
                            false => {
                                let body = response.json::<Value>().await?;
                                log::error!("Asset sent ERROR: {body}");
                            }
                        },
                        Err(e) => log::error!("Failed to send asset: {e:?}"),
                    }
                }
            }
            _ => return Err(eyre!("Output target not supported")),
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
