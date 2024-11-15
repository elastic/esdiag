use crate::data::{
    diagnostic::{logstash::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct LogstashVersion {
    host: String,
    version: String,
    http_address: String,
    id: String,
    pub name: String,
    ephemeral_id: String,
    status: String,
    snapshot: bool,
    pipeline: Pipeline,
    build_date: String,
    build_sha: String,
    build_snapshot: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct Pipeline {
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
}

impl DataSource for LogstashVersion {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("logstash_version.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("/"),
            _ => Err(eyre!("Unsupported source for Logstash version")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Version)
    }
}
