use super::{Product, Source};
use crate::output::Target;
use crate::setup::Asset;
use include_dir::{include_dir, Dir};
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub static ASSETS_DIR: Dir = include_dir!("assets");

pub fn read_string(file_path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    log::debug!("Reading file: {:?}", file_path);
    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(e) => return Err(Box::new(e)),
    };
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut string = String::new();
    while let Some(line) = lines.next() {
        string.push_str(&line?);
    }
    Ok(string)
}

pub fn parse_sources_yml(
    product: &Product,
) -> Result<HashMap<String, Source>, Box<dyn std::error::Error>> {
    log::debug!("Parsing sources.yml");
    let file = match product {
        Product::Elasticsearch => ASSETS_DIR
            .get_file("elasticsearch/sources.yml")
            .expect("No sources.yml file found!"),
        _ => unimplemented!("Application not yet implemented for sources!"),
    };
    let sources: Result<HashMap<String, Source>, serde_yaml::Error> =
        serde_yaml::from_slice(file.contents());
    Ok(sources?)
}

pub fn parse_assets_yml(target: &Target) -> Result<Vec<Asset>, Box<dyn std::error::Error>> {
    let file = match target {
        Target::Elasticsearch(_) => ASSETS_DIR
            .get_file("elasticsearch/assets.yml")
            .expect("No assets.yml file found!"),
        _ => return Err("Application not implemented".into()),
    };
    let assets: Result<Vec<Asset>, serde_yaml::Error> =
        match serde_yaml::from_slice(file.contents()) {
            Ok(assets) => Ok(assets),
            Err(e) => Err(e),
        };
    Ok(assets?)
}
