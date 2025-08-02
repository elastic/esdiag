use super::super::super::diagnostic::{DataSource, data_source::PathType};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct Node {
    // Omitted duplicate metadata fields from deserialization
    #[serde(skip_serializing)]
    pipelines: Option<HashMap<String, Pipeline>>,
    os: Os,
    jvm: Jvm,
}

impl Node {
    pub fn get_pipeline_count(&self) -> u32 {
        match self.pipelines {
            Some(ref pipelines) => pipelines.len() as u32,
            None => 0,
        }
    }

    pub fn take_pipelines(&mut self) -> HashMap<String, Pipeline> {
        match self.pipelines.take() {
            Some(pipelines) => pipelines,
            None => HashMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Pipeline {
    ephemeral_id: String,
    hash: String,
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
    config_reload_automatic: bool,
    config_reload_interval: u64,
    dead_letter_queue_enabled: bool,
    // Not in source file
    name: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Os {
    name: String,
    arch: String,
    version: String,
    available_processors: u32,
}

#[derive(Deserialize, Serialize)]
struct Jvm {
    pid: u32,
    version: String,
    vm_version: String,
    vm_vendor: String,
    vm_name: String,
    start_time_in_millis: u64,
    mem: Memory,
    gc_collectors: Vec<String>,
}

#[derive(Deserialize, Serialize)]
struct Memory {
    heap_init_in_bytes: u64,
    heap_max_in_bytes: u64,
    non_heap_init_in_bytes: u64,
    non_heap_max_in_bytes: u64,
}

impl DataSource for Node {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("logstash_node.json"),
            PathType::Url => Ok("_node"),
        }
    }

    fn name() -> String {
        "node".to_string()
    }
}
