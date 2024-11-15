use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
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
    attributes: Option<Value>,
    breakers: Value,
    pub discovery: Value,
    pub fs: Filesystem,
    host: Option<Value>,
    pub http: Value,
    indexing_pressure: Value,
    indices: Value,
    pub ingest: Ingest,
    ip: Option<Value>,
    jvm: Value,
    name: Value,
    os: OsStats,
    process: Value,
    repositories: Option<Value>,
    pub roles: Vec<String>,
    script: Value,
    script_cache: Value,
    thread_pool: Value,
    pub transport: Option<Value>,
    transport_address: Option<Value>,
    timestamp: Option<usize>,
}

#[derive(Deserialize, Serialize)]
struct LoadAverage {
    #[serde(rename = "1m")]
    one: f64,
    #[serde(rename = "5m")]
    five: f64,
    #[serde(rename = "15m")]
    fifteen: f64,
}

#[derive(Deserialize, Serialize)]
struct LoadPercent {
    #[serde(rename = "1m")]
    one: usize,
    #[serde(rename = "5m")]
    five: usize,
    #[serde(rename = "15m")]
    fifteen: usize,
}

impl Default for LoadPercent {
    fn default() -> Self {
        LoadPercent {
            one: 0,
            five: 0,
            fifteen: 0,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct OsStats {
    timestamp: usize,
    cpu: CpuStats,
    mem: Value,
    swap: Option<Value>,
    cgroup: Option<Value>,
}

#[derive(Deserialize, Serialize)]
pub struct CpuStats {
    percent: usize,
    load_average: LoadAverage,
    #[serde(skip_deserializing)]
    load_percent: LoadPercent,
}

impl NodeStats {
    pub fn calculate_stats(&mut self, processors: usize) {
        self.fs.total.used_in_bytes = self.fs.total.total_in_bytes - self.fs.total.free_in_bytes;
        self.fs.total.used_percent =
            (self.fs.total.used_in_bytes * 100) / self.fs.total.total_in_bytes;
        self.os.cpu.load_percent.one = (self.os.cpu.load_average.one * 100.0) as usize / processors;
        self.os.cpu.load_percent.five =
            (self.os.cpu.load_average.five * 100.0) as usize / processors;
        self.os.cpu.load_percent.fifteen =
            (self.os.cpu.load_average.fifteen * 100.0) as usize / processors;
    }
}

#[derive(Deserialize, Serialize)]
pub struct Filesystem {
    timestamp: Option<u64>,
    pub total: FilesystemTotal,
    //data: Vec<Value>,
    io_stats: Option<IoStats>,
}

#[derive(Deserialize, Serialize)]
struct IoStats {
    //devices: Vec<Value>,
    total: Option<Value>,
}

#[derive(Deserialize, Serialize)]
pub struct FilesystemTotal {
    // available: String,
    pub available_in_bytes: usize,
    // free: String,
    pub free_in_bytes: usize,
    // total: String,
    pub total_in_bytes: usize,
    #[serde(skip_deserializing)]
    pub used_in_bytes: usize,
    #[serde(skip_deserializing)]
    pub used_percent: usize,
}

pub type IngestPipelines = HashMap<String, IngestPipeline>;

#[derive(Deserialize, Serialize)]
pub struct Ingest {
    total: Value,
    #[serde(skip_serializing)] // Docs split into separate datastream
    pub pipelines: Option<IngestPipelines>,
}

pub type IngestProcessors = Vec<HashMap<String, IngestProcessor>>;

#[derive(Deserialize, Serialize)]
pub struct IngestPipeline {
    count: u64,
    time_in_millis: u64,
    current: u64,
    failed: u64,
    #[serde(skip_serializing)] // Docs split into separate datastream
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

    fn name() -> String {
        format!("{}", DataSet::NodesStats)
    }
}
