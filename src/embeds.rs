// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use rust_embed::RustEmbed;

/// Assets in the `assets/` directory (e.g. setup assets, `marked.js`)
#[derive(RustEmbed)]
#[folder = "assets/"]
#[exclude = "kibana/**"]
pub struct Assets;

/// The portable ESDiag skill installed by `esdiag agent skills`.
///
/// Helper scripts are intentionally excluded: installed skills compose native
/// commands, so every binary carries one script-free, version-matched asset set.
#[cfg(feature = "agent")]
#[derive(RustEmbed)]
#[folder = ".agents/skills/esdiag/"]
#[include = "SKILL.md"]
#[include = "references/**"]
#[include = "agents/**"]
#[exclude = "scripts/**"]
pub struct EsdiagSkillAssets;

/// Generated Kibana asset bundle built from `assets/kibana`.
#[cfg(feature = "setup")]
pub static KIBANA_ASSETS_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kibana-assets.zip"));

/// Documentation assets in the `docs/` directory
#[cfg(feature = "server")]
#[derive(RustEmbed)]
#[folder = "docs/"]
pub struct DocsAssets;

/// Server frontend assets in the `src/server/assets/` directory
#[cfg(feature = "server")]
#[derive(RustEmbed)]
#[folder = "src/server/assets/"]
pub struct ServerAssets;
