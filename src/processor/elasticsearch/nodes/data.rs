// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use crate::processor::elasticsearch::syscalls::NodeRuntimeVars;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::{HashMap, HashSet};

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct Node {
    aggregations: Option<Box<RawValue>>,
    pub attributes: Option<Box<RawValue>>,
    build_flavor: String,
    build_hash: String,
    build_type: String,
    component_version: Option<ComponentVersion>,
    pub host: Option<String>,
    http: Option<Box<RawValue>>,
    index_version: Option<i64>,
    //ingest: Box<RawValue>,
    pub ip: Option<String>,
    jvm: Box<RawValue>,
    //modules: Box<RawValue>,
    pub name: String,
    pub os: OsDetails,
    plugins: Option<Box<RawValue>>,
    pub process: Box<RawValue>,
    pub role: Option<String>,
    pub roles: HashSet<String>,
    pub settings: Option<Box<RawValue>>,
    thread_pool: Box<RawValue>,
    total_indexing_buffer: Option<Box<RawValue>>,
    total_indexing_buffer_in_bytes: Option<Box<RawValue>>,
    transport: Option<Box<RawValue>>,
    transport_address: Option<String>,
    transport_version: Option<i64>,
    pub version: Option<String>,
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
    _nodes: Box<RawValue>,
    //cluster_name: String,
    pub nodes: HashMap<String, Node>,
}

impl DataSource for Nodes {
    fn name() -> String {
        "nodes".to_string()
    }
}

impl Node {
    pub fn runtime_vars(&self) -> NodeRuntimeVars {
        let mut vars = NodeRuntimeVars::default();

        if let Ok(process) = serde_json::from_str::<NodeProcess>(self.process.get()) {
            vars.pid = process.id.map(|id| id.to_string());
        }

        if let Some(settings) = &self.settings
            && let Ok(settings) = serde_json::from_str::<NodeSettings>(settings.get())
        {
            vars.log_path = settings.path.and_then(|path| path.logs);
            vars.cluster_name = settings.cluster.and_then(|cluster| cluster.name);
        }

        vars
    }
}

#[derive(Deserialize)]
struct NodeProcess {
    id: Option<i64>,
}

#[derive(Deserialize)]
struct NodeSettings {
    path: Option<NodePathSettings>,
    cluster: Option<NodeClusterSettings>,
}

#[derive(Deserialize)]
struct NodePathSettings {
    logs: Option<String>,
}

#[derive(Deserialize)]
struct NodeClusterSettings {
    name: Option<String>,
}
