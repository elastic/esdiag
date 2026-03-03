// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerState, patch_signals};
use async_stream::stream;
use axum::{
    extract::State,
    response::{IntoResponse, Sse},
};
use std::sync::Arc;

pub async fn handler(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    log::debug!("Started stats stream");
    Sse::new(stream! {
        let mut last_stats_json = String::new();
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let stats = state.stats.read().await;
            match serde_json::to_string(&*stats) {
                Ok(json) => {
                    if json != last_stats_json {
                        last_stats_json = json.clone();
                        yield patch_signals(&format!(r#"{{"stats":{}}}"#, json));
                    }
                }
                Err(err) => log::error!("Failed to serialize stats: {}", err),
            }
        }
    })
}
