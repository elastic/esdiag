use crate::data::{
    diagnostic::{data_source::DataSource, logstash::DataSet},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct LogstashHotThreads {
    host: String,
    version: String,
    http_address: String,
    id: String,
    name: String,
    ephemeral_id: String,
    status: String,
    snapshot: bool,
    pipeline: Pipeline,
    hot_threads: HotThreads,
}

#[derive(Deserialize, Serialize)]
struct Pipeline {
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
}

#[derive(Deserialize, Serialize)]
struct HotThreads {
    time: String,
    busiest_threads: u32,
    threads: Vec<ThreadInfo>,
}

#[derive(Deserialize, Serialize)]
struct ThreadInfo {
    name: String,
    thread_id: u32,
    percent_of_cpu_time: f32,
    state: String,
    traces: Vec<String>,
}

impl DataSource for LogstashHotThreads {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("logstash_nodes_hot_threads.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_node/hot_threads?threads=10000"),
            _ => Err(eyre!("Unsupported source for Logstash hot threads ")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::HotThreads)
    }
}
