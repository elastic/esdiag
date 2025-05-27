use crate::data::diagnostic::{DataSource, data_source::PathType, elasticsearch::DataSet};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IlmExplain {
    pub indices: HashMap<String, IlmStats>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IlmStats {
    //pub index: String,
    pub managed: bool,
    pub policy: Option<String>,
    pub index_creation_date_millis: Option<u64>,
    pub lifecycle_date_millis: Option<u64>,
    pub phase: Option<String>,
    pub phase_time_millis: Option<u64>,
    pub action: Option<String>,
    pub action_time_millis: Option<u64>,
    pub step: Option<String>,
    pub step_time_millis: Option<u64>,
    pub repository_name: Option<String>,
    pub snapshot_name: Option<String>,
    pub phase_execution: Option<PhaseExecution>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Actions {
    searchable_snapshot: Option<SearchableSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PhaseDefinition {
    min_age: String,
    actions: Actions,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseExecution {
    policy: String,
    phase_definition: Option<PhaseDefinition>,
    version: i32,
    modified_date_in_millis: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SearchableSnapshot {
    snapshot_repository: String,
    force_merge_index: bool,
}

impl DataSource for IlmExplain {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/ilm_explain.json"),
            PathType::Url => Ok("_all/_ilm/explain"),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::IlmExplain)
    }
}
