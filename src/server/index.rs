// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerState, get_theme_dark, template};
use crate::data::{HostRole, KnownHost, keystore_exists};
use crate::exporter::Exporter;
use crate::processor::api::ApiResolver;
use askama::Template;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{path::PathBuf, str::FromStr, sync::Arc};

#[allow(dead_code)] // Needed when deserializing signals to modify selected tab in Web UI
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    FileUpload,
    ServiceLink,
    ApiKey,
}

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tab::FileUpload => write!(f, "file_upload"),
            Tab::ServiceLink => write!(f, "service_link"),
            Tab::ApiKey => write!(f, "api_key"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Params {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    key_id: Option<u64>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    link_id: Option<u64>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    upload_id: Option<u64>,
}

/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

pub async fn handler(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("Authentication header validation failed: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Html(format!(
                    "<html><body><h1>Unauthorized</h1><p>{}</p></body></html>",
                    err
                )),
            )
                .into_response();
        }
    };
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();
    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let page = template::Index {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        kibana_url,
        key_id: params.key_id,
        link_id: params.link_id,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

pub async fn workflow_page(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("Authentication header validation failed: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Html(format!(
                    "<html><body><h1>Unauthorized</h1><p>{}</p></body></html>",
                    err
                )),
            )
                .into_response();
        }
    };
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();

    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let exporter = { state.exporter.read().await.clone() };
    let send_defaults = classify_configured_exporter(&exporter);
    let workflow_hosts = workflow_host_options(&state);
    let default_save_dir = default_downloads_dir().display().to_string();
    let process_options_json =
        serde_json::to_string(&ApiResolver::processing_catalog().unwrap_or_default())
            .unwrap_or_else(|_| "{}".to_string());
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let page = template::Workflow {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        collect_hosts: workflow_hosts.collect_hosts,
        collect_secure_hosts_json: serde_json::to_string(&workflow_hosts.collect_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        configured_local_path: send_defaults.local_path,
        configured_remote_target: send_defaults.remote_target,
        default_save_dir,
        initial_send_mode: send_defaults.mode,
        initial_local_target: send_defaults.local_target,
        initial_remote_target: send_defaults.remote_target_default,
        kibana_url,
        key_id: params.key_id,
        link_id: params.link_id,
        process_options_json,
        send_secure_hosts_json: serde_json::to_string(&workflow_hosts.send_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        send_local_hosts: workflow_hosts.send_local_hosts,
        send_remote_hosts: workflow_hosts.send_remote_hosts,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

pub async fn jobs_page(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("Authentication header validation failed: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Html(format!(
                    "<html><body><h1>Unauthorized</h1><p>{}</p></body></html>",
                    err
                )),
            )
                .into_response();
        }
    };
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();

    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let exporter = { state.exporter.read().await.clone() };
    let send_defaults = classify_configured_exporter(&exporter);
    let workflow_hosts = workflow_host_options(&state);
    let default_save_dir = default_downloads_dir().display().to_string();
    let process_options_json =
        serde_json::to_string(&ApiResolver::processing_catalog().unwrap_or_default())
            .unwrap_or_else(|_| "{}".to_string());
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let page = template::Jobs {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        collect_hosts: workflow_hosts.collect_hosts,
        collect_secure_hosts_json: serde_json::to_string(&workflow_hosts.collect_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        configured_local_path: send_defaults.local_path,
        configured_remote_target: send_defaults.remote_target,
        default_save_dir,
        initial_send_mode: send_defaults.mode,
        initial_local_target: send_defaults.local_target,
        initial_remote_target: send_defaults.remote_target_default,
        kibana_url,
        key_id: params.key_id,
        link_id: params.link_id,
        process_options_json,
        send_secure_hosts_json: serde_json::to_string(&workflow_hosts.send_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        send_local_hosts: workflow_hosts.send_local_hosts,
        send_remote_hosts: workflow_hosts.send_remote_hosts,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

struct SendDefaults {
    mode: String,
    local_path: String,
    local_target: String,
    remote_target: String,
    remote_target_default: String,
}

struct WorkflowHostOptions {
    collect_hosts: Vec<String>,
    collect_secure_hosts: Vec<String>,
    send_remote_hosts: Vec<String>,
    send_local_hosts: Vec<String>,
    send_secure_hosts: Vec<String>,
}

fn classify_configured_exporter(exporter: &Exporter) -> SendDefaults {
    let target_uri = exporter.target_uri();
    match exporter {
        Exporter::Elasticsearch(_) => SendDefaults {
            mode: "remote".to_string(),
            local_path: String::new(),
            local_target: String::new(),
            remote_target: target_uri.clone(),
            remote_target_default: target_uri,
        },
        Exporter::Directory(_) | Exporter::File(_) => SendDefaults {
            mode: "local".to_string(),
            local_path: target_uri,
            local_target: "directory".to_string(),
            remote_target: String::new(),
            remote_target_default: String::new(),
        },
        Exporter::Archive(_) | Exporter::Stream(_) => SendDefaults {
            mode: "remote".to_string(),
            local_path: String::new(),
            local_target: String::new(),
            remote_target: target_uri.clone(),
            remote_target_default: target_uri,
        },
    }
}

fn workflow_host_options(state: &Arc<ServerState>) -> WorkflowHostOptions {
    if !state.runtime_mode_policy.allows_host_management() {
        return WorkflowHostOptions {
            collect_hosts: Vec::new(),
            collect_secure_hosts: Vec::new(),
            send_remote_hosts: Vec::new(),
            send_local_hosts: Vec::new(),
            send_secure_hosts: Vec::new(),
        };
    }

    let names = KnownHost::list_all().unwrap_or_default();
    let mut collect_hosts = Vec::new();
    let mut collect_secure_hosts = Vec::new();
    let mut send_remote_hosts = Vec::new();
    let mut send_local_hosts = Vec::new();
    let mut send_secure_hosts = Vec::new();

    for name in names {
        let Some(host) = KnownHost::get_known(&name) else {
            continue;
        };

        if host.has_role(HostRole::Collect) {
            collect_hosts.push(name.clone());
            if host.requires_keystore_secret() {
                collect_secure_hosts.push(name.clone());
            }
        }

        if host.has_role(HostRole::Send) {
            send_remote_hosts.push(name.clone());
            if host.requires_keystore_secret() {
                send_secure_hosts.push(name.clone());
            }
            if host.get_url().host_str().is_some_and(is_local_host) {
                send_local_hosts.push(name.clone());
            }
        }
    }

    WorkflowHostOptions {
        collect_hosts,
        collect_secure_hosts,
        send_remote_hosts,
        send_local_hosts,
        send_secure_hosts,
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1")
}

fn default_downloads_dir() -> PathBuf {
    let home_dir = match std::env::consts::OS {
        "windows" => std::env::var("USERPROFILE").unwrap_or_default(),
        "linux" | "macos" => std::env::var("HOME").unwrap_or_default(),
        _ => String::new(),
    };

    let home_path = PathBuf::from(home_dir);
    if home_path.as_os_str().is_empty() {
        PathBuf::from("Downloads")
    } else {
        home_path.join("Downloads")
    }
}
