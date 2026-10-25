// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana fleet processor
//!
//! Inputs:
//! - `kibana_fleet_agents_1.json` (`/api/fleet/agents`) -> `settings-kibana.fleet-esdiag`
//! - `kibana_fleet_agent_policies_1.json` (`/api/fleet/agent_policies`) -> `settings-kibana.fleet-esdiag`
//! - `kibana_fleet_packages.json` (`/api/fleet/epm/packages`) -> `settings-kibana.fleet-esdiag`
//! - `kibana_fleet_agent_status.json` (`/api/fleet/agent_status`) -> `metrics-kibana.fleet-esdiag`

mod data;
mod processor;

pub use data::*;
