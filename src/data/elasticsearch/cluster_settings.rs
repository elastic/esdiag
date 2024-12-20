use crate::data::diagnostic::{data_source::PathType, elasticsearch::DataSet, DataSource};
use color_eyre::eyre::Result;
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
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("cluster_settings_defaults.json"),
            PathType::Url => Ok("_cluster/settings?flat_settings&include_defaults=true"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::ClusterSettings)
    }
}
