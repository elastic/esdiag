// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana security processor
//!
//! Inputs:
//! - `kibana_roles.json` (`/api/security/role`) -> `settings-kibana.security-esdiag`
//! - `kibana_user.json` (`/api/security/v1/users`) -> `settings-kibana.security-esdiag`
//! - `kibana_actions.json` (`/api/actions`) -> `settings-kibana.security-esdiag`

mod data;
mod processor;

pub use data::*;
