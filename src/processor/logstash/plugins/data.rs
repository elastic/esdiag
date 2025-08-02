use super::super::super::diagnostic::{DataSource, data_source::PathType};
use eyre::Result;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Plugins {
    // Omitted duplicate metadata fields from deserialization
    pub total: u32,
    pub plugins: Vec<Plugin>,
}

#[derive(Deserialize, Serialize)]
pub struct Plugin {
    name: String,
    version: String,
}

impl DataSource for Plugins {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("logstash_plugins.json"),
            PathType::Url => Ok("_node/plugins"),
        }
    }

    fn name() -> String {
        "plugins".to_string()
    }
}
