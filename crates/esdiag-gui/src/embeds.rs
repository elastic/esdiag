// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use rust_embed::RustEmbed;

/// Documentation assets in the `docs/` directory
#[derive(RustEmbed)]
#[folder = "$ESDIAG_DOCS_DIR/"]
pub struct DocsAssets;

/// Server frontend assets in the `src/server/assets/` directory
#[derive(RustEmbed)]
#[folder = "src/server/assets/"]
pub struct ServerAssets;
