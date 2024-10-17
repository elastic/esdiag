use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Deserialize, Serialize)]
pub struct Node {
    aggregations: Value,
    pub attributes: Value,
    build_flavor: String,
    build_hash: String,
    build_type: String,
    component_version: Option<ComponentVersion>,
    pub host: String,
    http: Value,
    index_version: Option<i64>,
    //ingest: Value,
    pub ip: String,
    jvm: Value,
    //modules: Value,
    pub name: String,
    pub os: Value,
    plugins: Value,
    process: Value,
    pub role: Option<String>,
    pub roles: Vec<String>,
    settings: Value,
    thread_pool: Value,
    total_indexing_buffer: Value,
    total_indexing_buffer_in_bytes: Value,
    transport: Value,
    transport_address: String,
    transport_version: Option<i64>,
    pub version: semver::Version,
}

#[derive(Clone, Deserialize, Serialize)]
struct ComponentVersion {
    ml_config_version: i64,
    transform_config_version: i64,
}

// Deserializing data structures

#[derive(Deserialize)]
pub struct Nodes {
    _nodes: Value,
    //cluster_name: String,
    pub nodes: HashMap<String, Node>,
}

impl DataSource for Nodes {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("nodes.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_nodes"),
            _ => Err(eyre!("Unsupported source for nodes")),
        }
    }

    fn name() -> &'static str {
        "nodes"
    }
}
