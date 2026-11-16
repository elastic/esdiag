// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Configure external systems for ESDiag: Elastic Stack assets, coding-agent
//! skills, and shell completions.

/// Shell completion script generation and install helpers
pub mod completions;
/// Resolve setup targets from an Elastic CLI context
#[cfg(feature = "setup")]
pub mod context;
#[cfg(feature = "setup")]
mod elasticsearch;
/// Resolve setup targets from environment variables and env files
#[cfg(feature = "setup")]
pub mod env;
#[cfg(feature = "setup")]
mod install;
#[cfg(feature = "setup")]
mod kibana;
#[cfg(feature = "setup")]
mod license;
/// The shared Elasticsearch, license, and Kibana setup stages
#[cfg(feature = "setup")]
pub mod pipeline;
/// Embedded agent skill installation
#[cfg(feature = "agent")]
pub mod skills;

#[cfg(feature = "setup")]
use elasticsearch::*;
#[cfg(feature = "setup")]
use install::*;
#[cfg(feature = "setup")]
pub use install::{
    ASSETS_FILE, AssetStatus, SOURCES_FILE, SetupReport, asset_statuses, assets, assets_installed, assets_report,
    kibana_assets_report,
};
#[cfg(feature = "setup")]
pub use license::*;

#[cfg(all(test, feature = "setup"))]
mod serverless_tests;
