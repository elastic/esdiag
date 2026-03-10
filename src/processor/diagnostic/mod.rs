// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Trait for receiving data from a source
pub mod data_source;
/// Data stream naming
pub mod data_stream_name;
/// Modern diagnostic bundle manifest file
pub mod diagnostic_manifest;
/// Diagnostic metada doc
pub mod doc;
/// Diagnostic lookup tables
pub mod lookup;
/// Legacy diagnostic bundle manifest file
pub mod manifest;
/// Diagnostic job report
pub mod report;

pub use data_source::DataSource;
pub use data_stream_name::DataStreamName;
pub use diagnostic_manifest::DiagnosticManifest;
pub use doc::DiagnosticMetadata;
pub use lookup::Lookup;
pub use manifest::Manifest;
pub use report::{DiagnosticReport, DiagnosticReportBuilder};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagPath {
    pub diag_type: String,
    pub diag_path: String,
}
