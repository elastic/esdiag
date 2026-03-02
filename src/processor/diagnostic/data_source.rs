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

#[derive(Debug)]
pub enum DataSourceError {
    UnsupportedVersion(Version),
    MissingSource(String, String),
}

impl std::fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "API not supported on target version {}", v),
            Self::MissingSource(product, name) => {
                write!(
                    f,
                    "Source configuration missing for product: {}, name: {}",
                    product, name
                )
            }
        }
    }
}

impl std::error::Error for DataSourceError {}

pub trait DataSource {
    fn name() -> String;
    fn aliases() -> Vec<&'static str> {
        Vec::new()
    }
    fn product() -> &'static str {
        "elasticsearch"
    }
    fn source(path: PathType, version: Option<&Version>) -> Result<String> {
        let name = Self::name();
        let aliases = Self::aliases();
        let (matched_name, source_conf) = get_source(Self::product(), &name, &aliases)?;
        match path {
            PathType::File => Ok(source_conf.get_file_path(matched_name)),
            PathType::Url => {
                let v = version.ok_or_else(|| eyre!("Version required for URL"))?;
                source_conf.get_url(v).map_err(Into::into)
            }
        }
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
    pub tags: Option<String>,
    pub versions: BTreeMap<String, String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            extension: Some(String::from(".json")),
            subdir: None,
            tags: None,
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

pub fn init_sources(override_path: Option<String>) -> Result<()> {
    let mut products = HashMap::new();

    let es_content = if let Some(path) = override_path {
        std::fs::read_to_string(&path)
            .map_err(|e| eyre!("Failed to read override sources file at {}: {}", path, e))?
    } else {
        include_str!("../../../assets/elasticsearch/sources.yml").to_string()
    };

    let es_sources: HashMap<String, Source> = serde_yaml::from_str(&es_content)
        .map_err(|e| eyre!("Failed to parse sources.yml: {}", e))?;

    products.insert("elasticsearch", es_sources);

    SOURCES
        .set(products)
        .map_err(|_| eyre!("Sources already initialized"))?;
    Ok(())
}

pub fn get_source<'a>(
    product: &str,
    name: &'a str,
    aliases: &[&'a str],
) -> std::result::Result<(&'a str, &'static Source), DataSourceError> {
    let sources = get_sources();
    let product_sources = sources
        .get(product)
        .ok_or_else(|| DataSourceError::MissingSource(product.to_string(), name.to_string()))?;
    if let Some(source) = product_sources.get(name) {
        return Ok((name, source));
    }
    for alias in aliases {
        if let Some(source) = product_sources.get(*alias) {
            return Ok((*alias, source));
        }
    }
    Err(DataSourceError::MissingSource(
        product.to_string(),
        name.to_string(),
    ))
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
        Err(DataSourceError::UnsupportedVersion(version.clone()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::get_sources;
    use semver::Version;

    #[test]
    fn test_semver_parsing_and_matching() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // Let's test a simple one, like aliases
        let alias = es_sources.get("cat_aliases").unwrap();

        let v_0_9 = Version::parse("0.9.0").unwrap();
        let v_5_0 = Version::parse("5.0.0").unwrap();
        let v_5_1_1 = Version::parse("5.1.1").unwrap();
        let v_6_0 = Version::parse("6.0.0").unwrap();

        assert_eq!(alias.get_url(&v_0_9).unwrap(), "/_cat/aliases?v");
        assert_eq!(alias.get_url(&v_5_0).unwrap(), "/_cat/aliases?v");
        assert_eq!(
            alias.get_url(&v_5_1_1).unwrap(),
            "/_cat/aliases?v&s=alias,index"
        );
        assert_eq!(
            alias.get_url(&v_6_0).unwrap(),
            "/_cat/aliases?v&s=alias,index"
        );
    }

    #[test]
    fn test_semver_snapshots() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // snapshot should strip prerelease
        let ilm = es_sources.get("ilm_explain").unwrap();

        let v_8 = Version::parse("8.0.0-SNAPSHOT").unwrap();
        assert_eq!(
            ilm.get_url(&v_8).unwrap(),
            "/*/_ilm/explain?human&expand_wildcards=all"
        );
    }

    #[test]
    fn test_file_path_generation() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        let alias = es_sources.get("cat_aliases").unwrap();
        assert_eq!(alias.get_file_path("cat_aliases"), "cat/cat_aliases.txt");

        let tasks = es_sources.get("tasks").unwrap();
        assert_eq!(tasks.get_file_path("tasks"), "tasks.json"); // no subdir, default extension is json if missing from yaml
    }
}
