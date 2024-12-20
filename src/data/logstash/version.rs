use crate::data::diagnostic::{data_source::PathType, logstash::DataSet, DataSource};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    host: String,
    version: String,
    http_address: String,
    id: String,
    pub name: String,
    ephemeral_id: String,
    status: String,
    snapshot: bool,
    pipeline: Pipeline,
    #[serde(skip_serializing)]
    pub build_date: Option<String>,
    #[serde(skip_serializing)]
    pub build_sha: Option<String>,
    #[serde(skip_serializing)]
    pub build_snapshot: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct Pipeline {
    workers: u32,
    batch_size: u32,
    batch_delay: u32,
}

impl DataSource for Version {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("logstash_version.json"),
            PathType::Url => Ok("/"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Version)
    }
}
