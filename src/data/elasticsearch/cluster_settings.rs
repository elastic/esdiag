use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct ClusterSettings {
    pub transient: Value,
    pub persistent: Value,
    pub defaults: Value,
}

impl ClusterSettings {
    pub fn get_display_name(&self) -> Option<String> {
        self.persistent
            .get("cluster.metadata.display_name")
            .map(|v| v.as_str().unwrap().to_string())
    }
}

impl DataSource for ClusterSettings {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("cluster_settings_defaults.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_cluster/settings?include_defaults=true"),
            _ => Err(eyre!("Unsuppored source for cluster settings")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::ClusterSettings)
    }
}
