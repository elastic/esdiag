// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Integration tests that share one test binary.
//!
//! Tests that set process-wide environment variables stay in their own binaries
//! under `tests/`: spawned `esdiag` commands inherit the parent environment, so
//! sharing a process would leak another test's `HOME` or `ESDIAG_*` overrides.

#[path = "../common/mod.rs"]
mod common;

mod agent_cli_tests;
mod archive_lookup_enriched_processors_tests;
mod collection_tests;
mod eck_archive_processing_tests;
mod elastic_cli_context_e2e_tests;
mod elastic_cli_help_tests;
mod elastic_cli_keyring_tests;
mod elastic_cli_native_contract_tests;
mod elastic_cli_output_tests;
mod elastic_diag_wrapper_tests;
mod elasticsearch_asset_templates_tests;
mod elasticsearch_feedback_tests;
mod executor_cli_send_parity_tests;
mod host_cli_tests;
mod logstash_archive_processing_tests;
mod logstash_collection_tests;
mod provenance_writers_tests;
mod runtime_mode_desktop_settings_tests;
mod runtime_mode_web_tests;
mod server_ui_docs_theme_tests;
mod serverless_setup_tests;
mod service_link_parser_tests;
mod service_link_wait_tests;
mod sources_override_tests;
