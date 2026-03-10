// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataSource;
use serde::{Deserialize, Serialize};

#[allow(dead_code)] // Future use for processing hot threads data
#[derive(Deserialize, Serialize)]
pub struct NodeHotThreads {
    // Omitted duplicate metadata fields from deserialization
    hot_threads: HotThreads,
}

#[allow(dead_code)] // Future use for processing hot threads data
#[derive(Deserialize, Serialize)]
struct HotThreads {
    time: String,
    busiest_threads: u32,
    threads: Vec<Thread>,
}

#[allow(dead_code)] // Future use for processing hot threads data
#[derive(Deserialize, Serialize)]
struct Thread {
    name: String,
    thread_id: u32,
    percent_of_cpu_time: f32,
    state: String,
    traces: Vec<String>,
}

impl DataSource for NodeHotThreads {
    fn name() -> String {
        "hot_threads".to_string()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["logstash_nodes_hot_threads"]
    }
}
