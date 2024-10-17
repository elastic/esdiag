use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct NodesStats {
    _nodes: Value,
    //cluster_name: String,
    pub nodes: HashMap<String, NodeStats>,
}

#[derive(Deserialize, Serialize)]
pub struct NodeStats {
    #[serde(skip_serializing)] // Docs split into separate datastream
    pub adaptive_selection: Option<Value>,
    allocations: Option<Value>, // Only present on data nodes
    attributes: Value,
    breakers: Value,
    pub discovery: Value,
    fs: Value,
    host: Value,
    pub http: Value,
    indexing_pressure: Value,
    indices: Value,
    pub ingest: Ingest,
    ip: Value,
    jvm: Value,
    name: Value,
    os: Value,
    process: Value,
    repositories: Value,
    pub roles: Vec<String>,
    script: Value,
    script_cache: Value,
    thread_pool: Value,
    pub transport: Value,
    transport_address: Value,
    timestamp: i64,
}

pub type IngestPipelines = HashMap<String, IngestPipeline>;

#[derive(Deserialize, Serialize)]
pub struct Ingest {
    total: Value,
    #[serde(skip_serializing)]
    pub pipelines: Option<IngestPipelines>,
}

pub type IngestProcessors = Vec<HashMap<String, IngestProcessor>>;

#[derive(Deserialize, Serialize)]
pub struct IngestPipeline {
    count: u64,
    time_in_millis: u64,
    current: u64,
    failed: u64,
    #[serde(skip_serializing)]
    pub processors: Option<IngestProcessors>,
}

#[derive(Deserialize)]
pub struct IngestProcessor {
    pub r#type: String,
    pub stats: IngestProcessorStats,
}

#[derive(Deserialize, Serialize)]
pub struct IngestProcessorStats {
    count: u64,
    time_in_millis: u64,
    current: u64,
    failed: u64,
}

impl DataSource for NodesStats {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("nodes_stats.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_nodes/stats"),
            _ => Err(eyre!("Unsupported source for node stats")),
        }
    }

    fn name() -> &'static str {
        "nodes_stats"
    }
}
