// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, MappingStats, MappingSummary};
use eyre::Result;

impl From<MappingStats> for Lookup<MappingSummary> {
    fn from(mapping_stats: MappingStats) -> Self {
        let mut lookup = Lookup::new();
        for (index_name, summary) in mapping_stats.summaries() {
            lookup.add(summary).with_name(&index_name);
        }
        lookup
    }
}

impl From<Result<MappingStats>> for Lookup<MappingSummary> {
    fn from(mapping_stats: Result<MappingStats>) -> Self {
        match mapping_stats {
            Ok(stats) => Lookup::<MappingSummary>::from(stats),
            Err(e) => {
                log::warn!("Failed to parse MappingStats: {}", e);
                Lookup::new()
            }
        }
    }
}
