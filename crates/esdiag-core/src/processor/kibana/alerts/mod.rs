// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana alerts processor
//!
//! Inputs:
//! - `kibana_alerts_1.json` (`/api/alerts/_find`) -> `settings-kibana.alerts-esdiag`
//! - `kibana_alerts_health.json` (`/api/alerts/_health`) -> `metrics-kibana.alerts-esdiag`

mod data;
mod processor;

pub use data::*;
