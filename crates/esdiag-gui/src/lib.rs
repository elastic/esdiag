// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Web server and desktop wrapper of [`esdiag`](https://crates.io/crates/esdiag).
//! This is an implementation crate published in lockstep with `esdiag`; it has
//! no supported public API.

#[cfg(test)]
use esdiag_core::TestEnv;
use esdiag_core::{client, data, exporter, job, onboarding, processor, receiver, setup};

/// Desktop app that wraps the embedded web server in a Tauri window
#[cfg(feature = "desktop")]
pub mod desktop;
/// Embedded web and documentation assets
pub mod embeds;
/// Serve the ESDiag http API
pub mod server;
