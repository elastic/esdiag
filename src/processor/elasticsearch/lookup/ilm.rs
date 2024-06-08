use super::{Identifiers, Lookup};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IlmExplain {
    indices: HashMap<String, IlmData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IlmData {
    pub index: String,
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
pub struct PhaseExecution {
    policy: String,
    phase_definition: Option<PhaseDefinition>,
    version: i32,
    modified_date_in_millis: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseDefinition {
    min_age: String,
    actions: Actions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actions {
    searchable_snapshot: Option<SearchableSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SearchableSnapshot {
    snapshot_repository: String,
    force_merge_index: bool,
}

impl From<String> for Lookup<IlmData> {
    fn from(string: String) -> Self {
        let ilm_explain: IlmExplain =
            serde_json::from_str(&string).expect("Failed to deserialize ilm_explain");

        let mut lookup_ilm: Lookup<IlmData> = Lookup::new();
        for (index, ilm_data) in ilm_explain.indices {
            lookup_ilm.insert(
                Identifiers {
                    id: None,
                    name: Some(index.clone()),
                    host: None,
                    ip: None,
                },
                ilm_data,
            )
        }
        lookup_ilm
    }
}
