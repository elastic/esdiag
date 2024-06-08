use super::manifest::Manifest;
use super::{Product, Source};
use crate::output::Target;
use crate::setup::Asset;
use core::panic;
use include_dir::{include_dir, Dir};
use serde_json::Value;
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub static ASSETS_DIR: Dir = include_dir!("assets");

pub fn parse_json(file_path: &PathBuf) -> Result<Value, Box<dyn std::error::Error>> {
    log::debug!("Parsing file: {:?}", file_path);
    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(e) => return Err(Box::new(e)),
    };
    let reader = BufReader::new(file);
    let json = serde_json::from_reader(reader);
    match json {
        Ok(json) => Ok(json),
        Err(e) => Err(Box::new(e)),
    }
}

pub fn read_first_line(file_path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    log::debug!("Reading file first line: {:?}", file_path);
    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(e) => return Err(Box::new(e)),
    };
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    match lines.next() {
        Some(line) => Ok(line?),
        None => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No lines found",
        ))),
    }
}

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

pub fn parse_manifest(dir: &PathBuf) -> Result<Manifest, Box<dyn std::error::Error>> {
    let file_path = dir.as_path().join("manifest.json");
    let manifest = match File::open(&file_path) {
        Ok(file) => {
            log::info!("Parsing manifest.json {:?}", &file_path);
            let reader = BufReader::new(file);
            match serde_json::from_reader(reader) {
                Ok(manifest) => manifest,
                Err(e) => {
                    panic!("ERROR: Failed to parse manifest.json file - {}", e);
                }
            }
        }
        Err(_) => {
            log::warn!("Failed to parse manifest.json file, falling back to version.json");
            let file_path = dir.as_path().join("version.json");
            log::info!("Parsing version.json {:?}", &file_path);
            let version = match File::open(&file_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    match serde_json::from_reader(reader) {
                        Ok(version) => version,
                        Err(e) => {
                            panic!("ERROR: Failed to parse version.json file - {}", e);
                        }
                    }
                }
                Err(e) => panic!("ERROR: No version.json file - {}", e),
            };
            let date = std::fs::metadata(&file_path)?.created()?;
            Manifest::from_es_version(version, date)
        }
    };
    Ok(manifest)
}

pub fn parse_sources_yml(
    product: &Product,
) -> Result<HashMap<String, Source>, Box<dyn std::error::Error>> {
    log::info!("Parsing sources.yml");
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
