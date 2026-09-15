// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana synthetics and uptime processor
//!
//! Inputs:
//! - `kibana_synthetics_monitor_filters.json` (`/api/synthetics/monitor_filters`) -> `settings-kibana.synthetics-esdiag`
//! - `kibana_uptime_locations.json` (`/api/uptime/locations`) -> `settings-kibana.uptime-esdiag`

mod data;
mod processor;

pub use data::*;
