// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, SnapshotRepositories};
use eyre::Result;
use serde_json::Value;

impl From<SnapshotRepositories> for Lookup<Value> {
    fn from(mut repositories: SnapshotRepositories) -> Self {
        let mut lookup: Lookup<Value> = Lookup::new();
        repositories.drain().for_each(|(name, config)| {
            lookup.add(config).with_name(&name);
        });
        log::debug!("lookup snapshot repository entries: {}", lookup.len());
        lookup
    }
}

impl From<Result<SnapshotRepositories>> for Lookup<Value> {
    fn from(repositories: Result<SnapshotRepositories>) -> Self {
        match repositories {
            Ok(repositories) => Lookup::<Value>::from_parsed(repositories),
            Err(e) => {
                log::warn!("Failed to parse SnapshotRepositories: {}", e);
                Lookup::new()
            }
        }
    }
}
