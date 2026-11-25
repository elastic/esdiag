// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Module paths of the `esdiag` binary and integration tests, re-exported from
//! the `esdiag-core` and `esdiag-gui` implementation crates. New library code
//! belongs in those crates.

pub use esdiag_core::*;
#[cfg(feature = "server")]
pub use esdiag_gui::server;
pub mod logging;
