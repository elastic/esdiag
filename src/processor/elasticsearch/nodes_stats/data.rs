// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::StreamingDataSource;
use super::super::DataSource;
use crate::data::option_map_as_vec_entries;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_with::skip_serializing_none;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc::Sender;

#[derive(Deserialize, Serialize)]
pub struct NodesStats {
    _nodes: NodeCount,
    //cluster_name: String,
    pub nodes: HashMap<String, NodeStats>,
}

impl StreamingDataSource for NodesStats {
    type Item = (String, NodeStats);

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct NodesStatsVisitor {
            sender: Sender<Result<(String, NodeStats)>>,
        }

        impl<'de> serde::de::Visitor<'de> for NodesStatsVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("NodesStats object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    if key == "nodes" {
                        map.next_value_seed(NodesMapVisitor {
                            sender: self.sender.clone(),
                        })?;
                    } else {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(())
            }
        }

        struct NodesMapVisitor {
            sender: Sender<Result<(String, NodeStats)>>,
        }

        impl<'de> serde::de::DeserializeSeed<'de> for NodesMapVisitor {
            type Value = ();
            fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_map(self)
            }
        }

        impl<'de> serde::de::Visitor<'de> for NodesMapVisitor {
            type Value = ();
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("nodes map")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<NodeStats>()?;
                    if self.sender.blocking_send(Ok((key, value))).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            }
        }

        deserializer.deserialize_map(NodesStatsVisitor { sender })
    }
}

#[derive(Deserialize, Serialize)]
pub struct NodeCount {
    total: u32,
    successful: u32,
    failed: u32,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct NodeStats {
    #[serde(skip_serializing)] // Docs split into separate datastream
    pub adaptive_selection: Option<Box<RawValue>>,
    allocations: Option<Box<RawValue>>, // Only present on data nodes
    attributes: Option<Box<RawValue>>,
    breakers: Box<RawValue>,
    pub discovery: Box<RawValue>,
    pub fs: Filesystem,
    host: Option<Box<RawValue>>,
    pub http: Box<RawValue>,
    indexing_pressure: Box<RawValue>,
    indices: Box<RawValue>,
    pub ingest: Ingest,
    ip: Option<Box<RawValue>>,
    jvm: Box<RawValue>,
    name: Box<RawValue>,
    os: OsStats,
    process: Box<RawValue>,
    repositories: Option<Box<RawValue>>,
    pub roles: HashSet<String>,
    script: Box<RawValue>,
    script_cache: Box<RawValue>,
    thread_pool: Box<RawValue>,
    pub transport: Option<Box<RawValue>>,
    transport_address: Option<Box<RawValue>>,
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
    mem: Box<RawValue>,
    swap: Option<Box<RawValue>>,
    cgroup: Option<Box<RawValue>>,
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
    //devices: Vec<Box<RawValue>>,
    total: Option<Box<RawValue>>,
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

pub type IngestPipelines = Vec<(String, IngestPipeline)>;

#[derive(Deserialize, Serialize)]
pub struct Ingest {
    total: Box<RawValue>,
    #[serde(
        default,
        deserialize_with = "option_map_as_vec_entries",
        skip_serializing
    )]
    pub pipelines: Option<Vec<(String, IngestPipeline)>>,
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

    fn name() -> String {
        "nodes_stats".to_string()
    }
}
