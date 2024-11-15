use crate::data::{
    diagnostic::{logstash::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct LogstashPlugins {
    // Omitted duplicate metadata fields from deserialization
    total: u32,
    plugins: Vec<Plugin>,
}

#[derive(Deserialize, Serialize, Debug)]
struct Plugin {
    name: String,
    version: String,
}

impl DataSource for LogstashPlugins {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("logstash_plugins.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_node/plugins"),
            _ => Err(eyre!("Unsupported source for Logstash plugins ")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Plugins)
    }
}
