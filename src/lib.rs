// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Shared client libraries for remote connections
pub mod client;
/// Data structures and types for serializing and deserializing
pub mod data;
/// Environment variables
pub mod env;
/// Exports data to various destinations
pub mod exporter;
/// Data transformation and processing logic
pub mod processor;
/// Receive data from various sources
pub mod receiver;
/// Serve the ESDiag http API
#[cfg(feature = "server")]
pub mod server;
/// Send pre-built assets (index templates, etc) to Elasticsearch
#[cfg(feature = "setup")]
pub mod setup;
