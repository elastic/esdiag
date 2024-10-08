use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IlmExplain {
    pub indices: HashMap<String, IlmStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IlmStats {
    //pub index: String,
    pub managed: bool,
    pub policy: Option<String>,
    pub index_creation_date_millis: Option<i64>,
    pub lifecycle_date_millis: Option<i64>,
    pub phase: Option<String>,
    pub phase_time_millis: Option<i64>,
    pub action: Option<String>,
    pub action_time_millis: Option<i64>,
    pub step: Option<String>,
    pub step_time_millis: Option<i64>,
    pub repository_name: Option<String>,
    pub snapshot_name: Option<String>,
    pub phase_execution: Option<PhaseExecution>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Actions {
    searchable_snapshot: Option<SearchableSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PhaseDefinition {
    min_age: String,
    actions: Actions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseExecution {
    policy: String,
    phase_definition: Option<PhaseDefinition>,
    version: i32,
    modified_date_in_millis: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SearchableSnapshot {
    snapshot_repository: String,
    force_merge_index: bool,
}

impl DataSource for IlmExplain {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("commercial/ilm_explain.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_ilm/explain"),
            _ => Err(eyre!("Unsuppored source for ILM explain")),
        }
    }
}
