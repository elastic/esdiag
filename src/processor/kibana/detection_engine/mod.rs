// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana detection engine processor
//!
//! Inputs:
//! - `kibana_detection_engine_health_cluster.json` (`/api/detection_engine/health`) -> `metrics-kibana.detection_engine-esdiag`
//! - `kibana_detection_engine_rules_installed_1.json` (`/api/detection_engine/rules/_find`) -> `settings-kibana.detection_engine-esdiag`

mod data;
mod processor;

pub use data::*;
