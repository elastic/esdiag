// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Web server and desktop wrapper of [`esdiag`](https://crates.io/crates/esdiag).
//! This is an implementation crate published in lockstep with `esdiag`; it has
//! no supported public API.

#[cfg(test)]
use esdiag_core::TestEnv;
#[cfg(feature = "keystore")]
use esdiag_core::onboarding;
#[cfg(all(feature = "keystore", feature = "setup"))]
use esdiag_core::{client, setup};
use esdiag_core::{data, exporter, job, operations, processor, receiver};

/// Desktop app that wraps the embedded web server in a Tauri window
#[cfg(feature = "desktop")]
pub mod desktop;
/// Embedded web and documentation assets
pub mod embeds;
/// Serve the ESDiag http API
pub mod server;
