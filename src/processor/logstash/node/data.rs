// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
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
        self.pipelines.take().unwrap_or_default()
    }
}

#[derive(Deserialize, Serialize)]
pub struct Pipeline {
    #[serde(skip_serializing_if = "Option::is_none")]
    ephemeral_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    config_reload_automatic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config_reload_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dead_letter_queue_enabled: Option<bool>,
    // Not in source file
    #[serde(skip_serializing_if = "Option::is_none")]
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
    fn name() -> String {
        "logstash_node".to_string()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["node"]
    }
}
