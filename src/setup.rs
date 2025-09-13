// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::exporter::Exporter;
use eyre::{Result, eyre};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use serde_json::{Value, from_slice};
use std::path::PathBuf;

// Subdirectory for templates and configs files
pub static ASSETS_DIR: Dir = include_dir!("assets");
pub static ELASTICSEARCH_ASSETS: &str = "elasticsearch/assets.yml";
pub static ELASTICSEARCH_SOURCES: &str = "elasticsearch/sources.yml";

#[derive(Deserialize)]
pub struct Asset {
    pub endpoint: String,
    //pub file: Option<String>,
    pub method: String,
    pub name: String,
    pub subdir: Option<String>,
    pub suffix: Option<String>,
}

/// Submit saved assets to the Elasticsearch APIs
pub async fn assets(exporter: Exporter) -> Result<()> {
    match exporter {
        Exporter::File(_) | Exporter::Stream(_) => {
            return Err(eyre!("Setup only supports Elasticsearch."));
        }
        _ => {}
    }

    // load asset list from ./assets/{product}/assets.yml
    let assets = parse_assets_yml(&exporter)?;

    for asset in assets {
        log::info!("Processing asset: {}", &asset.name);
        let dir_str = format!(
            "{}/{}",
            &exporter.as_str(),
            &asset.subdir.unwrap_or("".to_string())
        );
        let subdir = PathBuf::from(dir_str);
        let files = match ASSETS_DIR.get_dir(&subdir) {
            Some(dir) => dir.files(),
            None => return Err(eyre!("No assets directory found")),
        };

        // send assets to Elasticsearch
        match exporter {
            Exporter::Elasticsearch(ref exporter) => {
                // for each asset, send to Elasticsearch
                for file in files {
                    log::debug!("file.path: {:?}", &file.path());
                    let value: Option<Value> = match from_slice(file.contents()) {
                        Ok(value) => Some(value),
                        Err(e) => {
                            log::warn!("Failed to parse asset: {:?}", &e);
                            None
                        }
                    };
                    let stem = file.path().file_stem().unwrap().to_str().unwrap_or("");
                    let endpoint = format!(
                        "{}/{}{}",
                        &asset.endpoint,
                        &stem,
                        asset.suffix.clone().unwrap_or("".to_string()),
                    );
                    match exporter
                        .request(&asset.method, &endpoint, value.as_ref())
                        .await
                    {
                        Ok(response) => match response.status_code().is_success() {
                            true => {
                                log::info!(
                                    "{} {} {} {}",
                                    &asset.name,
                                    &stem,
                                    &asset.method,
                                    response.status_code()
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
fn parse_assets_yml(exporter: &Exporter) -> Result<Vec<Asset>> {
    let file = match exporter {
        Exporter::Elasticsearch(_) => ASSETS_DIR
            .get_file(ELASTICSEARCH_ASSETS)
            .ok_or(eyre!("Error reading {ELASTICSEARCH_ASSETS}"))?,
        _ => return Err(eyre!("Application not implemented")),
    };
    let assets = serde_yaml::from_slice(file.contents())?;
    Ok(assets)
}
