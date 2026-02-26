// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, IndexMapping, MappingStats, MappingSummary};
use eyre::Result;
use futures::stream::{BoxStream, StreamExt};

impl From<MappingStats> for Lookup<MappingSummary> {
    fn from(mapping_stats: MappingStats) -> Self {
        let mut lookup = Lookup::new();
        for (index_name, index_mapping) in mapping_stats.indices {
            lookup.add(index_mapping.summarize()).with_name(&index_name);
        }
        lookup
    }
}

impl Lookup<MappingSummary> {
    pub async fn from_stream(
        mut stream: BoxStream<'static, Result<(String, IndexMapping)>>,
    ) -> Self {
        let mut lookup = Lookup::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok((index_name, index_mapping)) => {
                    lookup.add(index_mapping.summarize()).with_name(&index_name);
                }
                Err(e) => {
                    log::warn!("Error reading from mapping stats stream: {}", e);
                }
            }
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
