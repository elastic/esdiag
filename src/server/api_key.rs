// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    ApiKeyFormSignals, ServerEvent, ServerState, WorkflowRunSignals, job_feed_event,
    receiver_stream, signal_event, template, template_event, workflow,
};
use crate::{
    data::{KnownHostBuilder, with_scoped_keystore_password},
    processor::new_job_id,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Sse},
};
use datastar::axum::ReadSignals;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

const DOWNLOAD_REJECTION_TTL: Duration = Duration::from_secs(300);

pub async fn form(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<ApiKeyFormSignals>,
) -> impl IntoResponse {
    // Extract authenticated user email from header
    let uri = signals.es_api.url.to_string();
    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_api_key_form(state, signals, uri, request_user, tx).await;
            });
        }
        Err(err) => {
            tokio::spawn(async move {
                state.record_failure().await;
                send_event(
                    &tx,
                    job_feed_event(template::JobFailed {
                        job_id: new_job_id(),
                        error: &format!("Unauthorized request: {}", err),
                        source: &uri,
                    }),
                )
                .await;
                send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            });
        }
    }
    Sse::new(receiver_stream(rx))
}

pub async fn id(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path(job_id): Path<u64>,
) -> impl IntoResponse {
    let (tx, rx) = mpsc::channel(64);
    match state.resolve_user_email(&headers) {
        Ok((_, request_user)) => {
            tokio::spawn(async move {
                run_api_key_id(state, job_id, request_user, tx).await;
            });
        }
        Err(err) => {
            tokio::spawn(async move {
                state.record_failure().await;
                send_event(
                    &tx,
                    template_event(template::JobFailed {
                        job_id,
                        error: &format!("Unauthorized request: {}", err),
                        source: "API key processing",
                    }),
                )
                .await;
                send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            });
        }
    }
    Sse::new(receiver_stream(rx))
}

async fn send_event(tx: &mpsc::Sender<ServerEvent>, event: ServerEvent) {
    let _ = tx.send(event).await;
}

pub(super) async fn run_api_key_form(
    state: Arc<ServerState>,
    signals: ApiKeyFormSignals,
    uri: String,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let download_token = signals.archive.download_token.clone();
    if let Err(err) = super::ensure_active_output_ready(&state).await {
        state
            .reject_retained_bundle(
                &download_token,
                &request_user,
                err.clone(),
                DOWNLOAD_REJECTION_TTL,
            )
            .await;
        state.record_failure().await;
        send_event(
            &tx,
            job_feed_event(template::JobFailed {
                job_id: new_job_id(),
                error: &err,
                source: "output target",
            }),
        )
        .await;
        send_event(&tx, signal_event(r#"{"loading":false,"processing":false}"#)).await;
        return;
    }

    let host = match KnownHostBuilder::new(signals.es_api.url.clone().into())
        .apikey(Some(signals.es_api.key.clone()))
        .build()
    {
        Ok(host) => host,
        Err(e) => {
            state
                .reject_retained_bundle(
                    &download_token,
                    &request_user,
                    format!("Failed to build host: {}", e),
                    DOWNLOAD_REJECTION_TTL,
                )
                .await;
            state.record_failure().await;
            let error_msg = format!("Failed to build host: {}", e);
            tracing::error!("Failed to build host: {}", e);
            send_event(
                &tx,
                job_feed_event(template::JobFailed {
                    job_id: new_job_id(),
                    error: &error_msg,
                    source: &uri,
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false,"processing":false}"#)).await;
            return;
        }
    };
    let job = super::WorkflowJob {
        identifiers: signals.metadata.clone(),
        input: super::WorkflowInput::FromRemoteHost {
            source: host.get_url().to_string(),
            host,
            diagnostic_type: signals.workflow.collect.diagnostic_type.clone(),
        },
    };
    let job_id = new_job_id();
    let keystore_password = state.keystore_password().await;
    if let Some(password) = keystore_password {
        with_scoped_keystore_password(password, async move {
            workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
        })
        .await;
    } else {
        workflow::run_job(state, signals.into(), job_id, request_user, tx, job, false).await;
    }
}

async fn run_api_key_id(
    state: Arc<ServerState>,
    job_id: u64,
    request_user: String,
    tx: mpsc::Sender<ServerEvent>,
) {
    let job = match state.pop_workflow_job(job_id).await {
        Some(job) => job,
        None => {
            send_event(
                &tx,
                template_event(template::JobFailed {
                    job_id,
                    error: &format!("API key id {} not found", job_id),
                    source: "API key processing",
                }),
            )
            .await;
            send_event(&tx, signal_event(r#"{"loading":false}"#)).await;
            return;
        }
    };
    workflow::run_job(
        state,
        WorkflowRunSignals::default(),
        job_id,
        request_user,
        tx,
        job,
        true,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::run_api_key_form;
    use crate::{
        data::{HostRole, KnownHost, Product, Settings, Uri, authenticate},
        exporter::Exporter,
        server::{ApiKeyFormSignals, ServerEvent, test_server_state},
    };
    use std::{collections::BTreeMap, sync::Mutex};
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let hosts_path = config_dir.join("hosts.yml");
        let keystore_path = config_dir.join("secrets.yml");
        let settings_path = config_dir.join("settings.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
            std::env::set_var("ESDIAG_SETTINGS", &settings_path);
        }
        tmp
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn run_api_key_form_rejects_locked_secure_output_before_job_start() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "secure-es".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                None,
                false,
                Some("secure-es".to_string()),
                None,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        Settings {
            active_target: Some("secure-es".to_string()),
            kibana_url: None,
        }
        .save()
        .expect("save settings");

        let state = test_server_state();
        *state.exporter.write().await = Exporter::try_from(KnownHost::new_no_auth(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("secure output uri"),
            vec![HostRole::Send],
            None,
            false,
        ))
        .expect("matching exporter");
        let mut signals = ApiKeyFormSignals::default();
        signals.archive.download_token = "token-1".to_string();
        signals.es_api.url =
            Uri::try_from("http://cluster.example:9200".to_string()).expect("api url");
        signals.es_api.key = "api-key".to_string();
        let (tx, mut rx) = mpsc::channel(8);

        run_api_key_form(
            state.clone(),
            signals,
            "http://cluster.example:9200".to_string(),
            "Anonymous".to_string(),
            tx,
        )
        .await;

        let mut saw_failure = false;
        let mut saw_terminal = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                ServerEvent::JobFeed(html)
                    if html.contains("output target")
                        && html.contains(
                            "Keystore is locked. Unlock it before processing secure outputs.",
                        ) =>
                {
                    saw_failure = true;
                }
                ServerEvent::Signals(payload)
                    if payload.contains(r#""loading":false"#)
                        && payload.contains(r#""processing":false"#) =>
                {
                    saw_terminal = true;
                }
                _ => {}
            }
        }

        assert!(saw_failure, "expected preflight failure job event");
        assert!(saw_terminal, "expected terminal loading signal");
        let retained = state
            .retained_bundle("token-1")
            .await
            .expect("retained bundle rejection");
        assert_eq!(
            retained.error.as_deref(),
            Some("Keystore is locked. Unlock it before processing secure outputs.")
        );
        assert_eq!(retained.owner, "Anonymous");
    }
}
