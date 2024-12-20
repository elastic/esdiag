#![allow(unreachable_patterns)] // supresses a warning about the `name` alias
use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Cluster {
    #[serde(skip_deserializing)]
    pub display_name: String,
    #[serde(alias = "name")]
    pub diagnostic_node: String,
    #[serde(alias = "cluster_name")]
    pub name: String,
    #[serde(alias = "cluster_uuid")]
    pub uuid: String,
    pub version: Version,
    #[serde(skip_serializing)]
    pub tagline: String,
}

impl Cluster {
    pub fn with_display_name(self, display_name: Option<String>) -> Self {
        let display_name = match display_name {
            Some(name) => {
                // Removes Elastic Cloud appended hash from name
                let stripped_name = regex::Regex::new(r"\(.*\)")
                    .unwrap()
                    .replace_all(&name, "")
                    .to_string();
                stripped_name.trim().to_string()
            }
            None => self.name.clone(),
        };

        Self {
            display_name,
            ..self
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    pub number: semver::Version,
    pub build_flavor: String,
    pub build_type: String,
    pub build_hash: String,
    pub build_date: String,
    pub build_snapshot: bool,
    pub lucene_version: String,
    pub minimum_wire_compatibility_version: String,
    pub minimum_index_compatibility_version: String,
}

impl DataSource for Cluster {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("version.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("/"),
            _ => Err(eyre!("Unsupported source for version")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Version)
    }
}
