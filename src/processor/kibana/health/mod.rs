// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana health processor
//!
//! Inputs:
//! - `kibana_task_manager_health.json` (`/api/task_manager/_health`) -> `metrics-kibana.task_manager-esdiag`
//! - `kibana_stack_monitoring_health.json` (`/api/stack_monitoring/_health`) -> `metrics-kibana.stack_monitoring-esdiag`

mod data;
mod processor;

pub use data::*;
