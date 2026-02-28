// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{eyre, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use tokio::sync::mpsc::Sender;

pub enum PathType {
    Url,
    File,
}

pub trait DataSource {
    fn source(path: PathType, version: Option<&Version>) -> Result<String>;
    fn name() -> String;
    fn product() -> &'static str {
        "elasticsearch"
    }
}

pub trait StreamingDataSource: DataSource {
    type Item: Send + 'static;
    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>;
}

#[allow(dead_code)] // For future use deserialzing the sources.yml
#[derive(Clone, PartialEq, Serialize, Deserialize, Eq)]
pub struct Source {
    pub extension: Option<String>,
    pub subdir: Option<String>,
    pub versions: BTreeMap<String, String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            extension: Some(String::from(".json")),
            subdir: None,
            versions: BTreeMap::new(),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.subdir {
            Some(subdir) => write!(fmt, "{}", subdir),
            None => Ok(()),
        }
    }
}

static SOURCES: OnceLock<HashMap<&'static str, HashMap<String, Source>>> = OnceLock::new();

pub fn get_sources() -> &'static HashMap<&'static str, HashMap<String, Source>> {
    SOURCES.get_or_init(|| {
        let mut products = HashMap::new();

        let es_sources: HashMap<String, Source> =
            serde_yaml::from_str(include_str!("../../../assets/elasticsearch/sources.yml"))
                .expect("Valid elasticsearch sources.yml");
        products.insert("elasticsearch", es_sources);

        // Add other products here as their sources.yml files become available.
        // E.g., kibana, logstash, etc.

        products
    })
}

pub fn get_source(product: &str, name: &str) -> Result<&'static Source> {
    let sources = get_sources();
    let product_sources = sources
        .get(product)
        .ok_or_else(|| eyre!("Product '{}' not found in sources config", product))?;
    product_sources
        .get(name)
        .ok_or_else(|| eyre!("Source '{}' not found for product '{}'", name, product))
}

fn convert_npm_semver_to_cargo(req: &str) -> String {
    let parts: Vec<&str> = req.split_whitespace().collect();
    let mut out = String::new();
    for i in 0..parts.len() {
        out.push_str(parts[i]);
        if i + 1 < parts.len() {
            // If current part starts with a digit and next starts with an operator, insert comma.
            if parts[i]
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
                && parts[i + 1].chars().next().map_or(false, |c| {
                    c == '<' || c == '>' || c == '=' || c == '~' || c == '^'
                })
            {
                out.push_str(", ");
            } else {
                out.push(' ');
            }
        }
    }
    out
}

impl Source {
    pub fn get_file_path(&self, name: &str) -> String {
        let extension = self.extension.as_deref().unwrap_or(".json");
        match &self.subdir {
            Some(subdir) => format!("{}/{}{}", subdir, name, extension),
            None => format!("{}{}", name, extension),
        }
    }

    pub fn get_url(&self, version: &Version) -> Result<String> {
        // Strip pre-release tags (like -SNAPSHOT) to ensure our broad semver matching logic
        // in sources.yml (e.g. ">= 7.0.0") matches properly. Standard semver treats ">= 7.0.0"
        // as NOT matching "8.0.0-SNAPSHOT" by default unless specifically asked to.
        let mut clean_version = version.clone();
        clean_version.pre = semver::Prerelease::EMPTY;

        for (req_str, url) in &self.versions {
            let cargo_req_str = convert_npm_semver_to_cargo(req_str);
            let req = VersionReq::parse(&cargo_req_str)
                .map_err(|e| eyre!("Failed to parse version req '{}': {}", req_str, e))?;
            if req.matches(&clean_version) {
                return Ok(url.clone());
            }
        }
        Err(eyre!("API not supported on target version {}", version))
    }
}
