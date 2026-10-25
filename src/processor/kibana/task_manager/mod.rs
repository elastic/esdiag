// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana task manager processor
//!
//! Inputs:
//! - `kibana_task_manager_health.json` (`/api/task_manager/_health`) -> `metrics-kibana.task_manager-esdiag`

mod data;
mod processor;

pub use data::*;
