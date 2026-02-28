// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

#![allow(unreachable_patterns)] // supresses a warning about the `name` alias
use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

#[skip_serializing_none]
#[derive(Clone, Deserialize)]
pub struct Cluster {
    #[serde(skip_deserializing)]
    pub display_name: String,
    #[serde(alias = "name")]
    pub diagnostic_node: String,
    #[serde(alias = "cluster_name")]
    pub name: String,
    #[serde(rename = "cluster_uuid")]
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

impl From<Cluster> for ClusterMetadata {
    fn from(cluster: Cluster) -> Self {
        Self {
            display_name: cluster.display_name,
            diagnostic_node: cluster.diagnostic_node,
            name: cluster.name,
            uuid: cluster.uuid,
            version: cluster.version,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ClusterMetadata {
    pub display_name: String,
    pub diagnostic_node: String,
    pub name: String,
    pub uuid: String,
    pub version: Version,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    pub number: semver::Version,
    pub build_flavor: Option<String>,
    pub build_type: String,
    pub build_hash: String,
    pub build_date: String,
    pub build_snapshot: bool,
    pub lucene_version: String,
    pub minimum_wire_compatibility_version: String,
    pub minimum_index_compatibility_version: String,
}

impl DataSource for Cluster {

    fn name() -> String {
        "version".to_string()
    }
}
