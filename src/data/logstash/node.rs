use crate::data::{
    diagnostic::{data_source::DataSource, logstash::DataSet},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct LogstashNode {
    // Omitted duplicate metadata fields from deserialization
    pipelines: HashMap<String, Pipeline>,
    os: Os,
    jvm: Jvm,
}

#[derive(Deserialize, Serialize)]
struct Pipeline {
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

impl DataSource for LogstashNode {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("logstash_node.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_node"),
            _ => Err(eyre!("Unsupported source for Logstash node")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Node)
    }
}
