use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde_json::Value;

pub type ClusterSettings = Value;

impl DataSource for ClusterSettings {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("cluster_settings_defaults.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_cluster/settings?include_defaults=true"),
            _ => Err(eyre!("Unsuppored source for cluster settings")),
        }
    }
}
