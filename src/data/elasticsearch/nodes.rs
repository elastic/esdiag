use crate::data::diagnostic::{DataSource, data_source::PathType, elasticsearch::DataSet};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;
use std::collections::HashMap;

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct Node {
    aggregations: Option<Value>,
    pub attributes: Option<Value>,
    build_flavor: String,
    build_hash: String,
    build_type: String,
    component_version: Option<ComponentVersion>,
    pub host: Option<String>,
    http: Option<Value>,
    index_version: Option<i64>,
    //ingest: Value,
    pub ip: Option<String>,
    jvm: Value,
    //modules: Value,
    pub name: String,
    pub os: OsDetails,
    plugins: Option<Value>,
    process: Value,
    pub role: Option<String>,
    pub roles: Vec<String>,
    settings: Option<Value>,
    thread_pool: Value,
    total_indexing_buffer: Option<Value>,
    total_indexing_buffer_in_bytes: Option<Value>,
    transport: Option<Value>,
    transport_address: Option<String>,
    transport_version: Option<i64>,
    pub version: Option<semver::Version>,
}

#[derive(Clone, Deserialize, Serialize)]
struct ComponentVersion {
    ml_config_version: i64,
    transform_config_version: i64,
}

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct OsDetails {
    pub refresh_interval_in_millis: usize,
    pub name: Option<String>,
    pub pretty_name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
    pub available_processors: usize,
    pub allocated_processors: usize,
}

#[derive(Deserialize, Serialize)]
pub struct Nodes {
    _nodes: Value,
    //cluster_name: String,
    pub nodes: HashMap<String, Node>,
}

impl DataSource for Nodes {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("nodes.json"),
            PathType::Url => Ok("_nodes"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Nodes)
    }
}
