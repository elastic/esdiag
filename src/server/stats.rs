// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerEvent, ServerState, signal_event};
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn spawn_stats_publisher(state: Arc<ServerState>, tx: broadcast::Sender<ServerEvent>) {
    tokio::spawn(async move {
        run_stats_events_loop(state, tx).await;
    });
}

async fn run_stats_events_loop(state: Arc<ServerState>, tx: broadcast::Sender<ServerEvent>) {
    let mut shutdown = state.shutdown_receiver();
    let mut stats_updates = state.stats_updates_receiver();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    break;
                }
            }
            changed = stats_updates.changed() => {
                if changed.is_err() {
                    break;
                }
                let stats = state.stats.read().await;
                match serde_json::to_string(&*stats) {
                    Ok(json) => {
                        let _ = tx.send(signal_event(format!(r#"{{"stats":{}}}"#, json)));
                    }
                    Err(err) => {
                        log::error!("Failed to serialize stats: {}", err);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run_stats_events_loop;
    use crate::server::test_server_state;
    use tokio::sync::broadcast;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn stats_publisher_emits_on_stats_change() {
        let state = test_server_state();
        let (tx, mut rx) = broadcast::channel(4);
        let handle = tokio::spawn(run_stats_events_loop(state.clone(), tx));
        state.record_success(1, 0).await;
        let first = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("receive timeout")
            .expect("event should be present");

        match first {
            crate::server::ServerEvent::Signals(payload) => {
                assert!(payload.contains(r#""stats""#));
            }
            _ => panic!("expected signals payload"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn stats_publisher_allows_disconnect_and_resume() {
        let state = test_server_state();
        let (tx, mut first_rx) = broadcast::channel(8);
        let handle = tokio::spawn(run_stats_events_loop(state.clone(), tx.clone()));

        // First subscriber receives an update, then disconnects.
        state.record_success(1, 0).await;
        let _first_event = timeout(Duration::from_secs(2), first_rx.recv())
            .await
            .expect("first receive timeout")
            .expect("first event should be present");
        drop(first_rx);

        // New subscriber attaches after disconnect.
        let mut resumed_rx = tx.subscribe();

        // A subsequent stats mutation should still be published.
        state.record_failure().await;
        let resumed_event = timeout(Duration::from_secs(2), resumed_rx.recv())
            .await
            .expect("resumed receive timeout")
            .expect("resumed event should be present");

        match resumed_event {
            crate::server::ServerEvent::Signals(payload) => {
                assert!(payload.contains(r#""stats""#));
            }
            _ => panic!("expected stats signals payload"),
        }

        handle.abort();
    }
}
