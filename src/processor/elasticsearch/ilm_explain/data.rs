// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
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
    pub failed_step: Option<String>,
    pub is_auto_retryable_error: Option<bool>,
    pub failed_step_retry_count: Option<u32>,
    pub step_info: Option<StepInfo>,
    pub previous_step_info: Option<StepInfo>,
    pub version: Option<u32>,
    pub modified_date_in_millis: Option<u64>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepInfo {
    pub r#type: Option<String>,
    pub reason: Option<String>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PhaseDefinition {
    min_age: Option<String>,
    actions: Option<Box<serde_json::value::RawValue>>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseExecution {
    policy: String,
    phase_definition: Option<PhaseDefinition>,
    version: i32,
    modified_date_in_millis: u64,
}

impl DataSource for IlmExplain {
    fn name() -> String {
        "ilm_explain".to_string()
    }
}
