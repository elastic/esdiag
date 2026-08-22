// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{Lookup, missing_source_error},
    IndexMapping, MappingStats, MappingSummary,
};
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
    pub async fn from_stream(mut stream: BoxStream<'static, Result<(String, IndexMapping)>>) -> Self {
        let mut lookup = Lookup::new();
        let mut saw_missing_source = false;
        let mut saw_error = false;
        while let Some(result) = stream.next().await {
            match result {
                Ok((index_name, index_mapping)) => {
                    lookup.add(index_mapping.summarize()).with_name(&index_name);
                }
                Err(e) => {
                    if missing_source_error(&e) {
                        tracing::debug!("Mapping stats source is absent: {}", e);
                        saw_missing_source = true;
                    } else {
                        tracing::warn!("Error reading from mapping stats stream: {}", e);
                        saw_error = true;
                    }
                }
            }
        }

        if lookup.is_empty() && saw_missing_source && !saw_error {
            Lookup::missing()
        } else if saw_error {
            lookup
        } else {
            lookup.was_parsed()
        }
    }
}

impl From<Result<MappingStats>> for Lookup<MappingSummary> {
    fn from(mapping_stats: Result<MappingStats>) -> Self {
        match mapping_stats {
            Ok(stats) => Lookup::<MappingSummary>::from(stats),
            Err(e) => {
                tracing::warn!("Failed to parse MappingStats: {}", e);
                Lookup::new()
            }
        }
    }
}
