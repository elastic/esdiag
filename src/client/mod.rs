// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Authentication methods
mod auth;
/// Wrapper for building Elasticsearch connections
mod elasticsearch;
/// Manage saving and loading hosts from a YAML file
mod known_host;

pub use auth::{Auth, AuthType};
pub use elasticsearch::ElasticsearchBuilder;
pub use known_host::{ElasticCloud, KnownHost, KnownHostBuilder};
