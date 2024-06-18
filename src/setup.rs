use crate::input::file;
use crate::output::{Output, Target};
use serde::Deserialize;
use serde_json::{from_slice, Value};
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct Asset {
    pub endpoint: String,
    pub file: Option<String>,
    pub method: String,
    pub name: String,
    pub subdir: Option<String>,
    pub suffix: Option<String>,
}

pub async fn assets(output: Output) -> Result<(), Box<dyn std::error::Error>> {
    // load asset list from ./assets/{product}/assets.yml
    let assets = match file::parse_assets_yml(&output.target) {
        Ok(assets) => assets,
        Err(e) => {
            log::error!("Failed to parse assets.yml: {:?}", &e);
            Err(e)?
        }
    };
    match output.test().await {
        Ok(body) => log::debug!("Elasticsearch response: {body}"),
        Err(e) => {
            log::error!("Elasticsearch connection: FAILED {}", e);
            std::process::exit(1);
        }
    }

    for asset in assets {
        log::info!("Processing asset: {}", &asset.name);
        let dir_str = format!(
            "{}/{}",
            &output.target,
            &asset.subdir.unwrap_or("".to_string())
        );
        let subdir = PathBuf::from(dir_str);
        let files = file::ASSETS_DIR
            .get_dir(&subdir)
            .expect(&format!("No assets directory found: {:?}", subdir))
            .files();

        // send assets to Elasticsearch
        match output.target {
            Target::Elasticsearch(ref client) => {
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
                    let response = client.send_asset(&endpoint, &value, &asset.method).await;
                    match response {
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
            _ => log::error!("Output target not supported"),
        }
    }
    Ok(())
}
