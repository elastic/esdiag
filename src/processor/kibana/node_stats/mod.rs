// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana node stats processor
//!
//! Inputs:
//! - `kibana_stats.json` (`/api/stats`) -> `metrics-kibana.node-esdiag`

mod data;
mod processor;

pub use data::*;
