// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana settings processor
//!
//! Inputs:
//! - `kibana_fleet_settings.json` (`/api/fleet/settings`) -> `settings-kibana.fleet-esdiag`
//! - `kibana_uptime_settings.json` (`/api/uptime/settings`) -> `settings-kibana.uptime-esdiag`

mod data;
mod processor;

pub use data::*;
