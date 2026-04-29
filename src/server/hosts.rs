use super::{ServerState, get_theme_dark, template};
use crate::data::{
    HostRole, KnownHost, KnownHostBuilder, Product, SecretAuth, Settings, keystore_exists, list_secret_entries,
    remove_secret, resolve_secret_auth, upsert_secret_auth,
};
use askama::Template;
use axum::{
    extract::{Form, Json, Path, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
};
use datastar::{
    axum::ReadSignals, consts::ElementPatchMode, patch_elements::PatchElements, patch_signals::PatchSignals,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    sync::Arc,
};
use url::Url;

#[derive(Deserialize)]
pub struct HostUpsertForm {
    pub original_name: Option<String>,
    pub name: String,
    pub auth: String,
    pub app: String,
    pub url: String,
    pub url_template: Option<String>,
    pub roles: String,
    pub viewer: Option<String>,
    pub secret: Option<String>,
    pub accept_invalid_certs: Option<String>,
}

#[derive(Deserialize)]
pub struct HostDeleteForm {
    pub name: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct HostRecordPayload {
    #[serde(default)]
    pub original_name: Option<String>,
    pub name: String,
    pub auth: String,
    pub app: String,
    pub url: String,
    #[serde(default)]
    pub url_template: bool,
    pub roles: String,
    #[serde(default)]
    pub viewer: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub accept_invalid_certs: bool,
}

#[derive(Deserialize)]
pub struct SecretUpsertForm {
    pub original_secret_id: Option<String>,
    pub secret_id: String,
    pub auth_type: String,
    pub apikey: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize)]
pub struct ClusterUpsertForm {
    pub original_name: Option<String>,
    pub name: String,
    pub auth: String,
    pub secret: Option<String>,
    pub accept_invalid_certs: Option<String>,
    pub elasticsearch_url: String,
    pub kibana_url: String,
}

#[derive(Deserialize)]
pub struct SecretDeleteForm {
    pub secret_id: String,
}

#[derive(Default, Deserialize)]
pub(crate) struct HostsUiSignals {
    #[serde(default)]
    hosts: TableUiSignals,
}

#[derive(Default, Deserialize)]
pub(crate) struct SecretsUiSignals {
    #[serde(default)]
    secrets: TableUiSignals,
}

#[derive(Default, Deserialize)]
pub(crate) struct ClustersUiSignals {
    #[serde(default)]
    clusters: TableUiSignals,
}

#[derive(Default, Deserialize)]
struct TableUiSignals {
    #[serde(default)]
    rows: HashMap<String, TableRowSignals>,
}

#[derive(Default, Deserialize)]
struct TableRowSignals {
    #[serde(default)]
    draft: Option<Map<String, Value>>,
}

#[derive(Deserialize, Serialize)]
struct HostDraftSignal {
    #[serde(default)]
    original_name: Option<String>,
    name: String,
    auth: String,
    app: String,
    url: String,
    #[serde(default)]
    url_template: bool,
    roles: String,
    #[serde(default)]
    viewer: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    accept_invalid_certs: bool,
}

#[derive(Deserialize, Serialize)]
struct SecretDraftSignal {
    #[serde(default)]
    original_secret_id: Option<String>,
    secret_id: String,
    #[serde(rename = "authtype")]
    auth_type: String,
    #[serde(default)]
    apikey: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct ClusterDraftSignal {
    #[serde(default)]
    original_name: Option<String>,
    name: String,
    auth: String,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    accept_invalid_certs: bool,
    elasticsearch_url: String,
    kibana_url: String,
}

pub async fn page(State(state): State<Arc<ServerState>>, headers: HeaderMap) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::UNAUTHORIZED,
                Html(format!("<html><body><h1>Unauthorized</h1><p>{}</p></body></html>", err)),
            )
                .into_response();
        }
    };

    let user_initial = user_email.chars().next().unwrap_or('_').to_ascii_uppercase();
    let (keystore_locked, keystore_lock_time) = state.keystore_status().await;
    let can_use_keystore = cfg!(feature = "keystore") && state.server_policy.allows_local_runtime_features();
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);

    let (hosts, clusters) = read_host_and_cluster_rows();
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match current_keystore_password(&state)
            .await
            .and_then(|password| read_secret_rows(&password))
        {
            Ok((rows, ids)) => {
                state.touch_keystore_session().await;
                (rows, ids)
            }
            Err(err) => {
                tracing::warn!("Failed to list keystore secrets for /settings: {}", err);
                (Vec::new(), Vec::new())
            }
        }
    };
    let hosts_panel_html = render_hosts_panel(&hosts, &secret_ids, keystore_locked)
        .unwrap_or_else(|err| format!("<panel id=\"hosts-table-panel\"><div>Error: {err}</div></panel>"));
    let secrets_panel_html = render_secrets_panel(&secrets, keystore_locked)
        .unwrap_or_else(|err| format!("<panel id=\"secrets-table-panel\"><div>Error: {err}</div></panel>"));
    let clusters_panel_html = render_clusters_panel(&clusters, &secret_ids, keystore_locked)
        .unwrap_or_else(|err| format!("<panel id=\"clusters-table-panel\"><div>Error: {err}</div></panel>"));

    let page = template::HostsPage {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        kibana_url: state.kibana_url.read().await.clone(),
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark: get_theme_dark(&headers),
        runtime_mode: state.runtime_mode.to_string(),
        show_advanced: state.server_policy.allows_advanced(),
        show_job_builder: state.server_policy.allows_job_builder(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
        hosts_panel_html,
        secrets_panel_html,
        clusters_panel_html,
    };

    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
                err
            )),
        )
            .into_response(),
    }
}

pub async fn host_action(
    State(state): State<Arc<ServerState>>,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<HostsUiSignals>>,
) -> Response {
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);
    let editing_action = matches!(action.as_str(), "create" | "read" | "update");

    if editing_action && !state.is_keystore_unlocked().await {
        tracing::warn!(
            "Rejected host action '{}' for row '{}' because keystore is locked",
            action,
            id
        );
        return (StatusCode::PRECONDITION_FAILED, "Unlock keystore before editing hosts.").into_response();
    }

    if action == "create" {
        if id != "new" {
            tracing::warn!("Rejected host create with unexpected id '{}'", id);
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }

        let hosts = read_host_rows();
        let secret_ids = load_secret_ids(&state).await;
        let row_id = next_row_id(signal_state.map(|s| &s.hosts), hosts.len());
        let row = blank_host_row();

        return sse_response(vec![
            row_signal_patch("hosts", row_id, &host_draft(&row, None))
                .as_datastar_event()
                .to_string(),
            patch_host_error_event(""),
            patch_append_event(
                "#hosts-table-body",
                render_host_row(row_id, &row, &secret_ids, false, true, false),
            )
            .to_string(),
        ]);
    }

    let row_id = match id.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            tracing::warn!("Rejected host action '{}' with non-numeric id '{}'", action, id);
            return (StatusCode::BAD_REQUEST, "id must be numeric or 'new'").into_response();
        }
    };
    let hosts = read_host_rows();

    match action.as_str() {
        "read" => {
            let Some(host) = host_row_by_id(&hosts, row_id).cloned() else {
                tracing::warn!("Host read for row {} failed: row not found", row_id);
                return (StatusCode::NOT_FOUND, "host row not found").into_response();
            };
            let secret_ids = load_secret_ids(&state).await;
            sse_response(vec![
                row_signal_patch("hosts", row_id, &host_draft(&host, Some(&host.name)))
                    .as_datastar_event()
                    .to_string(),
                patch_host_error_event(""),
                patch_row_event(
                    &host_row_selector(row_id),
                    render_host_row(row_id, &host, &secret_ids, false, true, true),
                )
                .to_string(),
            ])
        }
        "cancel" => {
            let Some(host) = host_row_by_id(&hosts, row_id).cloned() else {
                tracing::warn!("Host cancel for transient row {} removed patched row", row_id);
                return clear_and_remove_row("hosts", &host_row_selector(row_id), row_id);
            };
            let secret_ids = load_secret_ids(&state).await;
            sse_response(vec![
                clear_row_signal_patch("hosts", row_id).as_datastar_event().to_string(),
                patch_host_error_event(""),
                patch_row_event(
                    &host_row_selector(row_id),
                    render_host_row(row_id, &host, &secret_ids, false, false, true),
                )
                .to_string(),
            ])
        }
        "update" => {
            let Some(draft) = host_draft_from_signals(signal_state, row_id) else {
                let message = "Missing host row draft signals.".to_string();
                tracing::warn!("Host update for row {} failed: {}", row_id, message);
                return sse_response(vec![patch_host_error_event(&message)]);
            };
            let host_name = draft.name.clone();
            match apply_upsert_host(&state, draft).await {
                Ok(_) => patch_host_row_saved_response(&state, row_id, &host_name).await,
                Err(err) => {
                    tracing::warn!("Host update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_host_error_event(&err)])
                }
            }
        }
        "delete" => delete_host_row(&hosts, row_id).await,
        _ => {
            tracing::warn!("Rejected unsupported host action '{}' for row {}", action, row_id);
            (
                StatusCode::BAD_REQUEST,
                "unsupported action; supported: create, read, cancel, update, delete",
            )
                .into_response()
        }
    }
}

pub async fn cluster_action(
    State(state): State<Arc<ServerState>>,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<ClustersUiSignals>>,
) -> Response {
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);
    let editing_action = matches!(action.as_str(), "create" | "read" | "update");

    if editing_action && !state.is_keystore_unlocked().await {
        tracing::warn!(
            "Rejected cluster action '{}' for row '{}' because keystore is locked",
            action,
            id
        );
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before editing diagnostic clusters.",
        )
            .into_response();
    }

    if action == "create" {
        if id != "new" {
            tracing::warn!("Rejected cluster create with unexpected id '{}'", id);
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }

        let clusters = read_host_and_cluster_rows().1;
        let secret_ids = load_secret_ids(&state).await;
        let row_id = next_row_id(signal_state.map(|s| &s.clusters), clusters.len());
        let row = blank_cluster_row();

        return sse_response(vec![
            row_signal_patch("clusters", row_id, &cluster_draft(&row, None))
                .as_datastar_event()
                .to_string(),
            patch_cluster_error_event(""),
            patch_append_event(
                "#clusters-table-body",
                render_cluster_row(row_id, &row, &secret_ids, false, true, false),
            )
            .to_string(),
        ]);
    }

    let row_id = match id.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            tracing::warn!("Rejected cluster action '{}' with non-numeric id '{}'", action, id);
            return (StatusCode::BAD_REQUEST, "id must be numeric or 'new'").into_response();
        }
    };
    let clusters = read_host_and_cluster_rows().1;

    match action.as_str() {
        "read" => {
            let Some(cluster) = cluster_row_by_id(&clusters, row_id).cloned() else {
                tracing::warn!("Cluster read for row {} failed: row not found", row_id);
                return (StatusCode::NOT_FOUND, "cluster row not found").into_response();
            };
            let secret_ids = load_secret_ids(&state).await;
            sse_response(vec![
                row_signal_patch("clusters", row_id, &cluster_draft(&cluster, Some(&cluster.name)))
                    .as_datastar_event()
                    .to_string(),
                patch_cluster_error_event(""),
                patch_row_event(
                    &cluster_row_selector(row_id),
                    render_cluster_row(row_id, &cluster, &secret_ids, false, true, true),
                )
                .to_string(),
            ])
        }
        "cancel" => {
            let Some(cluster) = cluster_row_by_id(&clusters, row_id).cloned() else {
                tracing::warn!("Cluster cancel for transient row {} removed patched row", row_id);
                return clear_and_remove_row("clusters", &cluster_row_selector(row_id), row_id);
            };
            let secret_ids = load_secret_ids(&state).await;
            sse_response(vec![
                clear_row_signal_patch("clusters", row_id)
                    .as_datastar_event()
                    .to_string(),
                patch_cluster_error_event(""),
                patch_row_event(
                    &cluster_row_selector(row_id),
                    render_cluster_row(row_id, &cluster, &secret_ids, false, false, true),
                )
                .to_string(),
            ])
        }
        "update" => {
            let Some(draft) = cluster_draft_from_signals(signal_state, row_id) else {
                let message = "Missing diagnostic cluster row draft signals.".to_string();
                tracing::warn!("Cluster update for row {} failed: {}", row_id, message);
                return sse_response(vec![patch_cluster_error_event(&message)]);
            };
            match apply_upsert_cluster(&state, draft).await {
                Ok(_) => patch_hosts_and_secrets_with_clear_response(&state, Some(("clusters", row_id))).await,
                Err(err) => {
                    tracing::warn!("Cluster update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_cluster_error_event(&err)])
                }
            }
        }
        "delete" => delete_cluster_row(&state, &clusters, row_id).await,
        _ => (
            StatusCode::BAD_REQUEST,
            "unsupported action; supported: create, read, cancel, update, delete",
        )
            .into_response(),
    }
}

pub async fn secret_action(
    State(state): State<Arc<ServerState>>,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<SecretsUiSignals>>,
) -> Response {
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);

    if action == "create" {
        if id != "new" {
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }
        if !state.is_keystore_unlocked().await {
            return (
                StatusCode::PRECONDITION_FAILED,
                "Unlock keystore before editing secrets.",
            )
                .into_response();
        }

        let secrets = current_keystore_password(&state)
            .await
            .and_then(|password| read_secret_rows(&password))
            .map(|(rows, _)| rows)
            .unwrap_or_default();
        let row_id = next_row_id(signal_state.map(|s| &s.secrets), secrets.len());
        let row = blank_secret_row();

        return sse_response(vec![
            row_signal_patch("secrets", row_id, &secret_draft(&row, None))
                .as_datastar_event()
                .to_string(),
            patch_secret_error_event(""),
            patch_append_event(
                "#secrets-table-body",
                render_secret_row(row_id, &row, true, false, false),
            )
            .to_string(),
        ]);
    }

    let row_id = match id.parse::<usize>() {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, "id must be numeric or 'new'").into_response(),
    };

    match action.as_str() {
        "read" => {
            let (secrets, _) = match current_keystore_password(&state)
                .await
                .and_then(|password| read_secret_rows(&password))
            {
                Ok(result) => result,
                Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
            };
            let Some(secret) = secret_row_by_id(&secrets, row_id).cloned() else {
                return (StatusCode::NOT_FOUND, "secret row not found").into_response();
            };
            sse_response(vec![
                row_signal_patch("secrets", row_id, &secret_draft(&secret, Some(&secret.secret_id)))
                    .as_datastar_event()
                    .to_string(),
                patch_secret_error_event(""),
                patch_row_event(
                    &secret_row_selector(row_id),
                    render_secret_row(row_id, &secret, true, true, false),
                )
                .to_string(),
            ])
        }
        "cancel" => {
            if !state.is_keystore_unlocked().await {
                let Some(secret) = secret_row_for_cancel_from_signals(signal_state, row_id) else {
                    return clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id);
                };
                return sse_response(vec![
                    clear_row_signal_patch("secrets", row_id)
                        .as_datastar_event()
                        .to_string(),
                    patch_secret_error_event(""),
                    patch_row_event(
                        &secret_row_selector(row_id),
                        render_secret_row(row_id, &secret, false, true, false),
                    )
                    .to_string(),
                ]);
            }

            let (secrets, _) = match current_keystore_password(&state)
                .await
                .and_then(|password| read_secret_rows(&password))
            {
                Ok(result) => result,
                Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
            };
            let Some(secret) = secret_row_by_id(&secrets, row_id).cloned() else {
                return clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id);
            };
            sse_response(vec![
                clear_row_signal_patch("secrets", row_id)
                    .as_datastar_event()
                    .to_string(),
                patch_secret_error_event(""),
                patch_row_event(
                    &secret_row_selector(row_id),
                    render_secret_row(row_id, &secret, false, true, false),
                )
                .to_string(),
            ])
        }
        "update" => {
            let Some(draft) = secret_draft_from_signals(signal_state, row_id) else {
                let message = "Missing secret row draft signals.".to_string();
                tracing::warn!("Secret update for row {} failed: {}", row_id, message);
                return sse_response(vec![patch_secret_error_event(&message)]);
            };
            match apply_upsert_secret(&state, draft).await {
                Ok(_) => patch_hosts_and_secrets_with_clear_response(&state, Some(("secrets", row_id))).await,
                Err(err) => {
                    tracing::warn!("Secret update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_secret_error_event(&err)])
                }
            }
        }
        "delete" => {
            let (secrets, _) = match current_keystore_password(&state)
                .await
                .and_then(|password| read_secret_rows(&password))
            {
                Ok(result) => result,
                Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
            };
            if secret_row_by_id(&secrets, row_id).is_some() {
                return (
                    StatusCode::BAD_REQUEST,
                    "Persisted secret deletion must use the secret_id endpoint.",
                )
                    .into_response();
            }
            clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id)
        }
        _ => (
            StatusCode::BAD_REQUEST,
            "unsupported action; supported: create, read, cancel, update, delete",
        )
            .into_response(),
    }
}

pub async fn upsert_host(State(state): State<Arc<ServerState>>, Form(form): Form<HostUpsertForm>) -> Response {
    match apply_upsert_host(&state, form).await {
        Ok(_) => patch_hosts_panel_response(&state).await,
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn create_host(State(state): State<Arc<ServerState>>, Json(payload): Json<HostRecordPayload>) -> Response {
    let form = HostUpsertForm {
        original_name: None,
        name: payload.name,
        auth: payload.auth,
        app: payload.app,
        url: payload.url,
        url_template: payload.url_template.then_some("true".to_string()),
        roles: payload.roles,
        viewer: payload.viewer,
        secret: payload.secret,
        accept_invalid_certs: payload.accept_invalid_certs.then_some("true".to_string()),
    };
    match apply_upsert_host(&state, form).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn update_host(State(state): State<Arc<ServerState>>, Json(payload): Json<HostRecordPayload>) -> Response {
    let form = HostUpsertForm {
        original_name: payload.original_name,
        name: payload.name,
        auth: payload.auth,
        app: payload.app,
        url: payload.url,
        url_template: payload.url_template.then_some("true".to_string()),
        roles: payload.roles,
        viewer: payload.viewer,
        secret: payload.secret,
        accept_invalid_certs: payload.accept_invalid_certs.then_some("true".to_string()),
    };
    match apply_upsert_host(&state, form).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_host(State(state): State<Arc<ServerState>>, Form(form): Form<HostDeleteForm>) -> Response {
    let mut hosts = KnownHost::parse_hosts_yml().map_err(to_message);
    if let Ok(ref mut hosts) = hosts {
        hosts.remove(form.name.trim());
        return match KnownHost::write_hosts_yml(hosts) {
            Ok(_) => patch_hosts_panel_response(&state).await,
            Err(err) => (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
        };
    }

    (StatusCode::BAD_REQUEST, hosts.err().unwrap_or_default()).into_response()
}

pub async fn upsert_secret(State(state): State<Arc<ServerState>>, Form(form): Form<SecretUpsertForm>) -> Response {
    match apply_upsert_secret(&state, form).await {
        Ok(_) => patch_hosts_and_secrets_response(&state).await,
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_secret(State(state): State<Arc<ServerState>>, Form(form): Form<SecretDeleteForm>) -> Response {
    delete_secret_id_response(&state, form.secret_id).await
}

pub async fn delete_secret_by_id(State(state): State<Arc<ServerState>>, Path(secret_id): Path<String>) -> Response {
    delete_secret_id_response(&state, secret_id).await
}

async fn delete_secret_id_response(state: &Arc<ServerState>, secret_id: String) -> Response {
    if !state.is_keystore_unlocked().await {
        return secret_delete_error_response(secret_id.trim(), "Unlock keystore before deleting secrets.");
    }

    let password = match current_keystore_password(state).await {
        Ok(password) => password,
        Err(err) => return secret_delete_error_response(secret_id.trim(), &err),
    };

    match remove_secret(secret_id.trim(), None, &password) {
        Ok(_) => {
            state.touch_keystore_session().await;
            patch_hosts_and_secrets_response(state).await
        }
        Err(err) => secret_delete_error_response(secret_id.trim(), &to_message(err)),
    }
}

fn read_host_and_cluster_rows() -> (Vec<template::HostsTableRow>, Vec<template::DiagnosticClusterTableRow>) {
    let hosts = KnownHost::parse_hosts_yml().unwrap_or_default();
    let mut rows = Vec::new();
    let mut clusters = Vec::new();
    let mut cluster_members = std::collections::BTreeSet::new();

    for (name, host) in &hosts {
        if let Some(cluster) = diagnostic_cluster_row(name, host, &hosts) {
            cluster_members.insert(name.clone());
            cluster_members.insert(format!("{name}-kb"));
            clusters.push(cluster);
        }
    }

    for (name, host) in hosts {
        if cluster_members.contains(&name) {
            continue;
        }
        rows.push(host_row_from_known_host(name, host));
    }

    (rows, clusters)
}

fn read_host_rows() -> Vec<template::HostsTableRow> {
    read_host_and_cluster_rows().0
}

fn host_row_from_known_host(name: String, host: KnownHost) -> template::HostsTableRow {
    let mut row = template::HostsTableRow {
        name,
        auth: "none".to_string(),
        app: host.app().to_string(),
        url: host.transport_display(),
        url_template: host.is_template(),
        roles: host
            .roles()
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        viewer: host.viewer().unwrap_or("").to_string(),
        accept_invalid_certs: host.accept_invalid_certs(),
        cloud_id: String::new(),
        secret: String::new(),
    };
    row.auth = host_auth_value(&host).to_string();
    row.cloud_id = host
        .cloud_id
        .as_ref()
        .map(std::string::ToString::to_string)
        .unwrap_or_default();
    row.secret = host.secret.clone().unwrap_or_default();
    row
}

fn diagnostic_cluster_row(
    name: &str,
    host: &KnownHost,
    hosts: &BTreeMap<String, KnownHost>,
) -> Option<template::DiagnosticClusterTableRow> {
    if host.app() != &Product::Elasticsearch || !host.has_role(HostRole::Send) {
        return None;
    }

    let kibana_name = format!("{name}-kb");
    if host.viewer()? != kibana_name.as_str() {
        return None;
    }
    let kibana_host = hosts.get(&kibana_name)?;
    if kibana_host.app() != &Product::Kibana || !kibana_host.has_role(HostRole::View) {
        return None;
    }

    if host_auth_value(host) != host_auth_value(kibana_host)
        || host_secret_value(host) != host_secret_value(kibana_host)
        || host.accept_invalid_certs() != kibana_host.accept_invalid_certs()
        || host_has_plaintext_auth(host)
        || host_has_plaintext_auth(kibana_host)
    {
        return None;
    }

    Some(template::DiagnosticClusterTableRow {
        name: name.to_string(),
        auth: host_auth_value(host).to_string(),
        secret: host_secret_value(host),
        accept_invalid_certs: host.accept_invalid_certs(),
        elasticsearch_url: host.get_url().ok()?.to_string(),
        kibana_url: kibana_host.get_url().ok()?.to_string(),
    })
}

fn host_auth_value(host: &KnownHost) -> &'static str {
    if host.secret.is_some() {
        "secret"
    } else if host.legacy_apikey.is_some() {
        "apikey"
    } else if host.legacy_username.is_some() || host.legacy_password.is_some() {
        "basic"
    } else {
        "none"
    }
}

fn host_secret_value(host: &KnownHost) -> String {
    host.secret.clone().unwrap_or_default()
}

fn host_has_plaintext_auth(host: &KnownHost) -> bool {
    host.legacy_apikey.is_some() || host.legacy_username.is_some() || host.legacy_password.is_some()
}

fn read_secret_rows(keystore_password: &str) -> Result<(Vec<template::SecretTableRow>, Vec<String>), String> {
    let entries = list_secret_entries(keystore_password).map_err(to_message)?;
    let mut rows = Vec::with_capacity(entries.len());
    let mut secret_ids = Vec::with_capacity(entries.len());

    for (secret_id, entry) in entries {
        let (auth_type, username) = if entry.apikey.is_some() {
            ("ApiKey", String::new())
        } else if let Some(basic) = entry.basic.as_ref() {
            ("Basic", basic.username.clone())
        } else {
            ("Unknown", String::new())
        };
        secret_ids.push(secret_id.clone());
        rows.push(template::SecretTableRow {
            secret_id,
            auth_type: auth_type.to_string(),
            username,
        });
    }

    Ok((rows, secret_ids))
}

fn render_hosts_panel(
    hosts: &[template::HostsTableRow],
    secret_ids: &[String],
    keystore_locked: bool,
) -> Result<String, String> {
    template::HostsTablePanelTemplate {
        keystore_locked,
        rows_html: hosts
            .iter()
            .enumerate()
            .map(|(idx, host)| render_host_row(idx + 1, host, secret_ids, keystore_locked, false, true))
            .collect::<Vec<_>>()
            .join("\n"),
    }
    .render()
    .map_err(to_message)
}

fn render_clusters_panel(
    clusters: &[template::DiagnosticClusterTableRow],
    secret_ids: &[String],
    keystore_locked: bool,
) -> Result<String, String> {
    template::DiagnosticClustersTablePanelTemplate {
        keystore_locked,
        rows_html: clusters
            .iter()
            .enumerate()
            .map(|(idx, cluster)| render_cluster_row(idx + 1, cluster, secret_ids, keystore_locked, false, true))
            .collect::<Vec<_>>()
            .join("\n"),
    }
    .render()
    .map_err(to_message)
}

fn render_secrets_panel(secrets: &[template::SecretTableRow], keystore_locked: bool) -> Result<String, String> {
    template::SecretsTablePanelTemplate {
        rows_html: secrets
            .iter()
            .enumerate()
            .map(|(idx, secret)| render_secret_row(idx + 1, secret, false, true, keystore_locked))
            .collect::<Vec<_>>()
            .join("\n"),
        keystore_locked,
    }
    .render()
    .map_err(to_message)
}

fn render_host_row(
    row_id: usize,
    host: &template::HostsTableRow,
    secret_ids: &[String],
    keystore_locked: bool,
    editing: bool,
    persisted: bool,
) -> String {
    template::HostsTableRowTemplate {
        row_id,
        host: host.clone(),
        secret_ids: secret_ids.to_vec(),
        keystore_locked,
        editing,
        persisted,
    }
    .render()
    .expect("render host row")
}

fn render_cluster_row(
    row_id: usize,
    cluster: &template::DiagnosticClusterTableRow,
    secret_ids: &[String],
    keystore_locked: bool,
    editing: bool,
    persisted: bool,
) -> String {
    template::DiagnosticClusterTableRowTemplate {
        row_id,
        cluster: cluster.clone(),
        secret_ids: secret_ids.to_vec(),
        keystore_locked,
        editing,
        persisted,
    }
    .render()
    .expect("render cluster row")
}

fn render_secret_row(
    row_id: usize,
    secret: &template::SecretTableRow,
    editing: bool,
    persisted: bool,
    keystore_locked: bool,
) -> String {
    template::SecretsTableRowTemplate {
        row_id,
        secret: secret.clone(),
        editing,
        persisted,
        keystore_locked,
    }
    .render()
    .expect("render secret row")
}

async fn load_table_panel_data(
    state: &Arc<ServerState>,
) -> (
    Vec<template::HostsTableRow>,
    Vec<template::DiagnosticClusterTableRow>,
    Vec<template::SecretTableRow>,
    Vec<String>,
    bool,
) {
    let (hosts, clusters) = read_host_and_cluster_rows();
    let keystore_locked = state.keystore_status().await.0;
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match current_keystore_password(state)
            .await
            .and_then(|password| read_secret_rows(&password))
        {
            Ok((rows, ids)) => {
                state.touch_keystore_session().await;
                (rows, ids)
            }
            Err(err) => {
                tracing::warn!("Failed to list keystore secrets for /settings patch: {}", err);
                (Vec::new(), Vec::new())
            }
        }
    };

    (hosts, clusters, secrets, secret_ids, keystore_locked)
}

fn patch_panel_event(selector: &str, html: String) -> String {
    PatchElements::new(html)
        .selector(selector)
        .mode(ElementPatchMode::Outer)
        .as_datastar_event()
        .to_string()
}

fn patch_row_event(selector: &str, html: String) -> String {
    PatchElements::new(html)
        .selector(selector)
        .mode(ElementPatchMode::Outer)
        .as_datastar_event()
        .to_string()
}

fn patch_append_event(selector: &str, html: String) -> String {
    PatchElements::new(html)
        .selector(selector)
        .mode(ElementPatchMode::Append)
        .as_datastar_event()
        .to_string()
}

fn host_error_html(message: &str) -> String {
    if message.trim().is_empty() {
        "<div id=\"hosts-table-error\"></div>".to_string()
    } else {
        let message = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        format!(
            "<div id=\"hosts-table-error\" class=\"error\" role=\"alert\"><p>{}</p></div>",
            message
        )
    }
}

fn patch_host_error_event(message: &str) -> String {
    PatchElements::new(host_error_html(message))
        .selector("#hosts-table-error")
        .mode(ElementPatchMode::Outer)
        .as_datastar_event()
        .to_string()
}

fn cluster_error_html(message: &str) -> String {
    if message.trim().is_empty() {
        "<div id=\"clusters-table-error\"></div>".to_string()
    } else {
        let message = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        format!(
            "<div id=\"clusters-table-error\" class=\"error\" role=\"alert\"><p>{}</p></div>",
            message
        )
    }
}

fn patch_cluster_error_event(message: &str) -> String {
    PatchElements::new(cluster_error_html(message))
        .selector("#clusters-table-error")
        .mode(ElementPatchMode::Outer)
        .as_datastar_event()
        .to_string()
}

fn secret_error_html(message: &str) -> String {
    if message.trim().is_empty() {
        "<div id=\"secrets-error-modal\"></div>".to_string()
    } else {
        let message = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let close_action = "const modal = document.getElementById('secrets-error-modal'); if (modal) modal.outerHTML = `<div id='secrets-error-modal'></div>`;";
        format!(
            "<div id=\"secrets-error-modal\" class=\"modal\"><div class=\"modal-content\"><button type=\"button\" class=\"close-button\" aria-label=\"Close secret error\" data-on:click=\"{}\"><icon-x></icon-x></button><h2>Secret action failed</h2><p>{}</p><form-actions><button type=\"button\" class=\"button\" data-on:click=\"{}\">Close</button></form-actions></div></div>",
            close_action, message, close_action
        )
    }
}

fn patch_secret_error_event(message: &str) -> String {
    PatchElements::new(secret_error_html(message))
        .selector("#secrets-error-modal")
        .mode(ElementPatchMode::Outer)
        .as_datastar_event()
        .to_string()
}

fn secret_delete_error_response(secret_id: &str, message: &str) -> Response {
    tracing::warn!("Secret delete for '{}' failed: {}", secret_id, message);
    sse_response(vec![patch_secret_error_event(message)])
}

fn sse_response(events: Vec<String>) -> Response {
    ([(CONTENT_TYPE, "text/event-stream")], events.join("\n\n")).into_response()
}

async fn patch_hosts_panel_response(state: &Arc<ServerState>) -> Response {
    let (hosts, _clusters, _secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => sse_response(vec![patch_panel_event("#hosts-table-panel", html)]),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

async fn patch_hosts_and_secrets_response(state: &Arc<ServerState>) -> Response {
    let (hosts, clusters, secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    let hosts_html = match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let secrets_html = match render_secrets_panel(&secrets, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let clusters_html = match render_clusters_panel(&clusters, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };

    sse_response(vec![
        patch_panel_event("#hosts-table-panel", hosts_html),
        patch_panel_event("#secrets-table-panel", secrets_html),
        patch_panel_event("#clusters-table-panel", clusters_html),
    ])
}

async fn patch_hosts_and_secrets_with_clear_response(
    state: &Arc<ServerState>,
    clear: Option<(&str, usize)>,
) -> Response {
    let (hosts, clusters, secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    let hosts_html = match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let secrets_html = match render_secrets_panel(&secrets, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let clusters_html = match render_clusters_panel(&clusters, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let mut events = Vec::new();
    if let Some((panel, row_id)) = clear {
        events.push(clear_row_signal_patch(panel, row_id).as_datastar_event().to_string());
    }
    events.push(patch_panel_event("#hosts-table-panel", hosts_html));
    events.push(patch_panel_event("#secrets-table-panel", secrets_html));
    events.push(patch_panel_event("#clusters-table-panel", clusters_html));
    sse_response(events)
}

async fn apply_upsert_host(state: &Arc<ServerState>, form: HostUpsertForm) -> Result<(), String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("Host name is required.".to_string());
    }

    let target = form.url.trim();
    if target.is_empty() {
        return Err("Host URL or URL template is required.".to_string());
    }
    let use_url_template = form.url_template.is_some();
    let app = parse_product(form.app.trim())?;
    let roles = parse_roles(&form.roles)?;
    let viewer = to_opt(form.viewer);
    let secret = to_opt(form.secret);
    let accept_invalid_certs = form.accept_invalid_certs.is_some();
    let auth = infer_auth_from_secret_selection(state, &secret, form.auth.trim()).await?;

    let mut builder = if use_url_template {
        KnownHostBuilder::new_template(target.to_string())
    } else {
        let url = Url::parse(target).map_err(|err| format!("Invalid URL: {err}"))?;
        KnownHostBuilder::new(url)
    }
    .product(app)
    .accept_invalid_certs(accept_invalid_certs)
    .roles(roles)
    .viewer(viewer);
    let host = match auth.as_str() {
        "none" => builder.build().map_err(to_message)?,
        "apikey" | "basic" | "secret" => {
            let secret_id =
                secret.ok_or_else(|| "Saved authenticated hosts require a secret reference.".to_string())?;
            builder = builder.secret(Some(secret_id));
            builder.build().map_err(to_message)?
        }
        _ => return Err("Auth type must be one of: none, apikey, basic, secret.".to_string()),
    };

    let mut hosts: BTreeMap<String, KnownHost> = KnownHost::parse_hosts_yml().map_err(to_message)?;
    let mut settings = Settings::load().map_err(to_message)?;
    let mut settings_changed = false;
    if let Some(original_name) = form.original_name {
        let original_name = original_name.trim();
        if !original_name.is_empty() && original_name != name {
            hosts.remove(original_name);
            if settings.active_target.as_deref() == Some(original_name) {
                settings.active_target = Some(name.to_string());
                settings_changed = true;
            }
        }
    }
    hosts.insert(name.to_string(), host);
    KnownHost::write_hosts_yml(&hosts).map_err(to_message)?;

    if settings
        .active_target
        .as_ref()
        .is_some_and(|target| !hosts.contains_key(target))
    {
        settings.active_target = hosts.keys().next().cloned();
        settings_changed = true;
    }
    if settings_changed {
        settings.save().map_err(to_message)?;
    }
    Ok(())
}

async fn apply_upsert_cluster(state: &Arc<ServerState>, form: ClusterUpsertForm) -> Result<(), String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("Diagnostic cluster name is required.".to_string());
    }

    let elasticsearch_url =
        Url::parse(form.elasticsearch_url.trim()).map_err(|err| format!("Invalid Elasticsearch URL: {err}"))?;
    let kibana_url = Url::parse(form.kibana_url.trim()).map_err(|err| format!("Invalid Kibana URL: {err}"))?;
    let secret = to_opt(form.secret);
    let accept_invalid_certs = form.accept_invalid_certs.is_some();
    let auth = infer_auth_from_secret_selection(state, &secret, form.auth.trim()).await?;

    let kibana_name = format!("{name}-kb");
    let elasticsearch_host = {
        let mut builder = KnownHostBuilder::new(elasticsearch_url)
            .product(Product::Elasticsearch)
            .accept_invalid_certs(accept_invalid_certs)
            .roles(vec![HostRole::Send])
            .viewer(Some(kibana_name.clone()));
        match auth.as_str() {
            "none" => builder.build().map_err(to_message)?,
            "apikey" | "basic" | "secret" => {
                let secret_id = secret
                    .clone()
                    .ok_or_else(|| "Saved authenticated clusters require a secret reference.".to_string())?;
                builder = builder.secret(Some(secret_id));
                builder.build().map_err(to_message)?
            }
            _ => return Err("Auth type must be one of: none, apikey, basic, secret.".to_string()),
        }
    };
    let kibana_host = {
        let mut builder = KnownHostBuilder::new(kibana_url)
            .product(Product::Kibana)
            .accept_invalid_certs(accept_invalid_certs)
            .roles(vec![HostRole::View]);
        match auth.as_str() {
            "none" => builder.build().map_err(to_message)?,
            "apikey" | "basic" | "secret" => {
                let secret_id = secret
                    .clone()
                    .ok_or_else(|| "Saved authenticated clusters require a secret reference.".to_string())?;
                builder = builder.secret(Some(secret_id));
                builder.build().map_err(to_message)?
            }
            _ => unreachable!(),
        }
    };

    let mut hosts: BTreeMap<String, KnownHost> = KnownHost::parse_hosts_yml().map_err(to_message)?;
    let mut settings = Settings::load().map_err(to_message)?;
    let mut settings_changed = false;

    if let Some(original_name) = form.original_name {
        let original_name = original_name.trim();
        if !original_name.is_empty() && original_name != name {
            let original_kibana_name = format!("{original_name}-kb");
            hosts.remove(original_name);
            hosts.remove(&original_kibana_name);

            if settings.active_target.as_deref() == Some(original_name) {
                settings.active_target = Some(name.to_string());
                settings_changed = true;
            } else if settings.active_target.as_deref() == Some(original_kibana_name.as_str()) {
                settings.active_target = Some(kibana_name.clone());
                settings_changed = true;
            }
        }
    }

    hosts.insert(name.to_string(), elasticsearch_host);
    hosts.insert(kibana_name.clone(), kibana_host);
    KnownHost::write_hosts_yml(&hosts).map_err(to_message)?;

    if settings
        .active_target
        .as_ref()
        .is_some_and(|target| !hosts.contains_key(target))
    {
        settings.active_target = hosts.keys().next().cloned();
        settings_changed = true;
    }
    if settings_changed {
        settings.save().map_err(to_message)?;
    }

    Ok(())
}

async fn apply_upsert_secret(state: &Arc<ServerState>, form: SecretUpsertForm) -> Result<(), String> {
    if !state.is_keystore_unlocked().await {
        return Err("Unlock keystore before editing secrets.".to_string());
    }

    let secret_id = form.secret_id.trim();
    if secret_id.is_empty() {
        return Err("secret_id is required.".to_string());
    }
    let password = current_keystore_password(state).await?;

    let auth = match form.auth_type.trim().to_ascii_lowercase().as_str() {
        "apikey" => {
            let apikey = to_opt(form.apikey).ok_or("apikey is required for ApiKey type.")?;
            SecretAuth::ApiKey { apikey }
        }
        "basic" => {
            let username = to_opt(form.username).ok_or("username is required for Basic type.")?;
            let password_value = to_opt(form.password).ok_or("password is required for Basic type.")?;
            SecretAuth::Basic {
                username,
                password: password_value,
            }
        }
        _ => return Err("auth_type must be either ApiKey or Basic.".to_string()),
    };

    if let Some(original_secret_id) = form.original_secret_id {
        let original_secret_id = original_secret_id.trim();
        if !original_secret_id.is_empty() && original_secret_id != secret_id {
            remove_secret(original_secret_id, None, &password).map_err(to_message)?;
        }
    }

    upsert_secret_auth(secret_id, auth, &password).map_err(to_message)?;
    state.touch_keystore_session().await;
    Ok(())
}

fn host_row_selector(row_id: usize) -> String {
    format!("#hosts-table-row-{row_id}")
}

fn cluster_row_selector(row_id: usize) -> String {
    format!("#clusters-table-row-{row_id}")
}

fn secret_row_selector(row_id: usize) -> String {
    format!("#secrets-table-row-{row_id}")
}

fn host_row_by_id(hosts: &[template::HostsTableRow], row_id: usize) -> Option<&template::HostsTableRow> {
    row_id.checked_sub(1).and_then(|idx| hosts.get(idx))
}

fn secret_row_by_id(secrets: &[template::SecretTableRow], row_id: usize) -> Option<&template::SecretTableRow> {
    row_id.checked_sub(1).and_then(|idx| secrets.get(idx))
}

fn cluster_row_by_id(
    clusters: &[template::DiagnosticClusterTableRow],
    row_id: usize,
) -> Option<&template::DiagnosticClusterTableRow> {
    row_id.checked_sub(1).and_then(|idx| clusters.get(idx))
}

fn blank_host_row() -> template::HostsTableRow {
    template::HostsTableRow {
        name: String::new(),
        auth: "none".to_string(),
        app: "Elasticsearch".to_string(),
        url: String::new(),
        url_template: false,
        roles: "collect".to_string(),
        viewer: String::new(),
        accept_invalid_certs: false,
        cloud_id: String::new(),
        secret: String::new(),
    }
}

fn blank_cluster_row() -> template::DiagnosticClusterTableRow {
    template::DiagnosticClusterTableRow {
        name: String::new(),
        auth: "none".to_string(),
        secret: String::new(),
        accept_invalid_certs: false,
        elasticsearch_url: String::new(),
        kibana_url: String::new(),
    }
}

fn blank_secret_row() -> template::SecretTableRow {
    template::SecretTableRow {
        secret_id: String::new(),
        auth_type: "ApiKey".to_string(),
        username: String::new(),
    }
}

fn row_signal_patch<T: Serialize>(panel: &str, row_id: usize, draft: &T) -> PatchSignals {
    PatchSignals::new(
        json!({
            panel: {
                "rows": {
                    row_id.to_string(): {
                        "draft": draft
                    }
                }
            }
        })
        .to_string(),
    )
}

fn clear_row_signal_patch(panel: &str, row_id: usize) -> PatchSignals {
    PatchSignals::new(
        json!({
            panel: {
                "rows": {
                    row_id.to_string(): Value::Null
                }
            }
        })
        .to_string(),
    )
}

fn host_draft(host: &template::HostsTableRow, original_name: Option<&str>) -> HostDraftSignal {
    HostDraftSignal {
        original_name: original_name.map(str::to_string),
        name: host.name.clone(),
        auth: host.auth.clone(),
        app: host.app.clone(),
        url: host.url.clone(),
        url_template: host.url_template,
        roles: host.roles.clone(),
        viewer: Some(host.viewer.clone()),
        secret: Some(host.secret.clone()),
        accept_invalid_certs: host.accept_invalid_certs,
    }
}

fn cluster_draft(cluster: &template::DiagnosticClusterTableRow, original_name: Option<&str>) -> ClusterDraftSignal {
    ClusterDraftSignal {
        original_name: original_name.map(str::to_string),
        name: cluster.name.clone(),
        auth: cluster.auth.clone(),
        secret: Some(cluster.secret.clone()),
        accept_invalid_certs: cluster.accept_invalid_certs,
        elasticsearch_url: cluster.elasticsearch_url.clone(),
        kibana_url: cluster.kibana_url.clone(),
    }
}

fn secret_draft(secret: &template::SecretTableRow, original_secret_id: Option<&str>) -> SecretDraftSignal {
    SecretDraftSignal {
        original_secret_id: original_secret_id.map(str::to_string),
        secret_id: secret.secret_id.clone(),
        auth_type: secret.auth_type.clone(),
        apikey: None,
        username: (!secret.username.is_empty()).then(|| secret.username.clone()),
        password: None,
    }
}

fn host_draft_from_signals(signals: Option<&HostsUiSignals>, row_id: usize) -> Option<HostUpsertForm> {
    let draft: HostDraftSignal = draft_from_rows(signals.map(|s| &s.hosts), row_id)?;
    Some(HostUpsertForm {
        original_name: draft.original_name,
        name: draft.name,
        auth: draft.auth,
        app: draft.app,
        url: draft.url,
        url_template: draft.url_template.then_some("true".to_string()),
        roles: draft.roles,
        viewer: draft.viewer,
        secret: draft.secret,
        accept_invalid_certs: draft.accept_invalid_certs.then_some("true".to_string()),
    })
}

fn cluster_draft_from_signals(signals: Option<&ClustersUiSignals>, row_id: usize) -> Option<ClusterUpsertForm> {
    let draft: ClusterDraftSignal = draft_from_rows(signals.map(|s| &s.clusters), row_id)?;
    Some(ClusterUpsertForm {
        original_name: draft.original_name,
        name: draft.name,
        auth: draft.auth,
        secret: draft.secret,
        accept_invalid_certs: draft.accept_invalid_certs.then_some("true".to_string()),
        elasticsearch_url: draft.elasticsearch_url,
        kibana_url: draft.kibana_url,
    })
}

fn secret_draft_from_signals(signals: Option<&SecretsUiSignals>, row_id: usize) -> Option<SecretUpsertForm> {
    let draft: SecretDraftSignal = draft_from_rows(signals.map(|s| &s.secrets), row_id)?;
    Some(SecretUpsertForm {
        original_secret_id: draft.original_secret_id,
        secret_id: draft.secret_id,
        auth_type: draft.auth_type.trim().to_string(),
        apikey: draft.apikey,
        username: draft.username,
        password: draft.password,
    })
}

fn secret_row_for_cancel_from_signals(
    signals: Option<&SecretsUiSignals>,
    row_id: usize,
) -> Option<template::SecretTableRow> {
    let draft = secret_draft_from_signals(signals, row_id)?;
    Some(template::SecretTableRow {
        secret_id: draft
            .original_secret_id
            .clone()
            .unwrap_or(draft.secret_id)
            .trim()
            .to_string(),
        auth_type: draft.auth_type.trim().to_string(),
        username: draft.username.unwrap_or_default(),
    })
}

fn draft_from_rows<T: for<'de> Deserialize<'de>>(signals: Option<&TableUiSignals>, row_id: usize) -> Option<T> {
    let row = signals?.rows.get(&row_id.to_string())?.draft.clone()?;
    serde_json::from_value(Value::Object(row)).ok()
}

fn next_row_id(signals: Option<&TableUiSignals>, existing_len: usize) -> usize {
    let signal_max = signals
        .map(|signals| {
            signals
                .rows
                .keys()
                .filter_map(|key| key.parse::<usize>().ok())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    existing_len.max(signal_max) + 1
}

async fn load_secret_ids(state: &Arc<ServerState>) -> Vec<String> {
    if !state.is_keystore_unlocked().await {
        return Vec::new();
    }
    match current_keystore_password(state)
        .await
        .and_then(|password| read_secret_rows(&password))
    {
        Ok((_, ids)) => {
            state.touch_keystore_session().await;
            ids
        }
        Err(err) => {
            tracing::warn!("Failed to read secret ids for hosts row render: {}", err);
            Vec::new()
        }
    }
}

async fn delete_host_row(hosts: &[template::HostsTableRow], row_id: usize) -> Response {
    let Some(host) = host_row_by_id(hosts, row_id) else {
        tracing::warn!("Host delete for transient row {} removed patched row", row_id);
        return clear_and_remove_row("hosts", &host_row_selector(row_id), row_id);
    };

    let mut host_map = KnownHost::parse_hosts_yml().map_err(to_message);
    if let Ok(ref mut host_map) = host_map {
        host_map.remove(host.name.trim());
        return match KnownHost::write_hosts_yml(host_map) {
            Ok(_) => sse_response(vec![
                clear_row_signal_patch("hosts", row_id).as_datastar_event().to_string(),
                patch_host_error_event(""),
                PatchElements::new_remove(host_row_selector(row_id))
                    .as_datastar_event()
                    .to_string(),
            ]),
            Err(err) => {
                let message = to_message(err);
                tracing::warn!("Host delete for row {} failed: {}", row_id, message);
                sse_response(vec![patch_host_error_event(&message)])
            }
        };
    }

    let message = host_map.err().unwrap_or_default();
    tracing::warn!("Host delete for row {} failed: {}", row_id, message);
    sse_response(vec![patch_host_error_event(&message)])
}

async fn delete_cluster_row(
    state: &Arc<ServerState>,
    clusters: &[template::DiagnosticClusterTableRow],
    row_id: usize,
) -> Response {
    let Some(cluster) = cluster_row_by_id(clusters, row_id) else {
        tracing::warn!("Cluster delete for transient row {} removed patched row", row_id);
        return clear_and_remove_row("clusters", &cluster_row_selector(row_id), row_id);
    };

    let mut host_map = KnownHost::parse_hosts_yml().map_err(to_message);
    if let Ok(ref mut host_map) = host_map {
        let kibana_name = format!("{}-kb", cluster.name.trim());
        host_map.remove(cluster.name.trim());
        host_map.remove(&kibana_name);

        let mut settings = match Settings::load() {
            Ok(settings) => settings,
            Err(err) => return sse_response(vec![patch_cluster_error_event(&to_message(err))]),
        };
        let mut settings_changed = false;
        if settings.active_target.as_deref() == Some(cluster.name.trim())
            || settings.active_target.as_deref() == Some(kibana_name.as_str())
        {
            settings.active_target = host_map.keys().next().cloned();
            settings_changed = true;
        }

        return match KnownHost::write_hosts_yml(host_map) {
            Ok(_) => {
                if settings_changed && let Err(err) = settings.save() {
                    let message = to_message(err);
                    tracing::warn!("Cluster delete for row {} failed: {}", row_id, message);
                    return sse_response(vec![patch_cluster_error_event(&message)]);
                }
                patch_hosts_and_secrets_with_clear_response(state, Some(("clusters", row_id))).await
            }
            Err(err) => {
                let message = to_message(err);
                tracing::warn!("Cluster delete for row {} failed: {}", row_id, message);
                sse_response(vec![patch_cluster_error_event(&message)])
            }
        };
    }

    let message = host_map.err().unwrap_or_default();
    tracing::warn!("Cluster delete for row {} failed: {}", row_id, message);
    sse_response(vec![patch_cluster_error_event(&message)])
}

fn clear_and_remove_row(panel: &str, selector: &str, row_id: usize) -> Response {
    sse_response(vec![
        clear_row_signal_patch(panel, row_id).as_datastar_event().to_string(),
        PatchElements::new_remove(selector).as_datastar_event().to_string(),
    ])
}

async fn patch_host_row_saved_response(state: &Arc<ServerState>, row_id: usize, host_name: &str) -> Response {
    let hosts = read_host_rows();
    let Some(host) = hosts.iter().find(|host| host.name == host_name).cloned() else {
        let message = format!("Saved host '{}' but could not reload the updated row.", host_name);
        tracing::warn!("{}", message);
        return sse_response(vec![patch_host_error_event(&message)]);
    };
    let keystore_locked = state.keystore_status().await.0;
    sse_response(vec![
        clear_row_signal_patch("hosts", row_id).as_datastar_event().to_string(),
        patch_host_error_event(""),
        patch_row_event(
            &host_row_selector(row_id),
            render_host_row(row_id, &host, &[], keystore_locked, false, true),
        ),
    ])
}

fn parse_roles(value: &str) -> Result<Vec<HostRole>, String> {
    let parsed: Result<Vec<HostRole>, String> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| HostRole::from_str(part).map_err(to_message))
        .collect();
    let parsed = parsed?;
    if parsed.is_empty() {
        Ok(vec![HostRole::Collect])
    } else {
        Ok(parsed)
    }
}

fn parse_product(value: &str) -> Result<Product, String> {
    let normalized = value.trim().to_ascii_lowercase();
    let mapped = match normalized.as_str() {
        "elasticcloudhosted" => "elastic-cloud-hosted",
        "kubernetesplatform" => "mki",
        other => other,
    };
    Product::from_str(mapped).map_err(|err| format!("Invalid product: {err}"))
}

async fn infer_auth_from_secret_selection(
    state: &Arc<ServerState>,
    secret: &Option<String>,
    fallback_auth: &str,
) -> Result<String, String> {
    if let Some(secret_id) = secret {
        if !state.is_keystore_unlocked().await {
            return Err("Unlock keystore before selecting a secret.".to_string());
        }
        let password = current_keystore_password(state).await?;
        let resolved = resolve_secret_auth(secret_id, &password).map_err(to_message)?;
        state.touch_keystore_session().await;
        return match resolved {
            Some(SecretAuth::ApiKey { .. }) => Ok("apikey".to_string()),
            Some(SecretAuth::Basic { .. }) => Ok("basic".to_string()),
            None => Err(format!("Unknown secret_id '{secret_id}'")),
        };
    }

    let normalized = fallback_auth.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        Ok("none".to_string())
    } else {
        Ok(normalized)
    }
}

fn to_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn to_message(err: impl std::fmt::Display) -> String {
    err.to_string()
}

async fn current_keystore_password(state: &Arc<ServerState>) -> Result<String, String> {
    state
        .keystore_password()
        .await
        .ok_or_else(|| "Keystore password is not available for the current session.".to_string())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::{
        ClusterUpsertForm, HostUpsertForm, SecretsUiSignals, TableRowSignals, TableUiSignals, apply_upsert_cluster,
        apply_upsert_host, delete_secret_by_id, host_draft, read_host_and_cluster_rows, read_secret_rows,
        render_host_row, render_secret_row, render_secrets_panel, secret_action,
    };
    use crate::{
        data::{
            HostRole, KnownHost, KnownHostBuilder, Product, SecretAuth, Settings, authenticate, upsert_secret_auth,
        },
        server::{template, test_server_state},
    };
    use axum::{
        body::to_bytes,
        extract::Path,
        http::{StatusCode, header::CONTENT_TYPE},
    };
    use datastar::axum::ReadSignals;
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Mutex};
    use tempfile::TempDir;
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
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
        }
        tmp
    }

    #[tokio::test]
    async fn renaming_active_host_updates_settings_target() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "old-host".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let settings = Settings {
            active_target: Some("old-host".to_string()),
            kibana_url: None,
        };
        settings.save().expect("save settings");

        let state = test_server_state();
        apply_upsert_host(
            &state,
            HostUpsertForm {
                original_name: Some("old-host".to_string()),
                name: "new-host".to_string(),
                auth: "none".to_string(),
                app: "Elasticsearch".to_string(),
                url: "http://localhost:9200".to_string(),
                url_template: None,
                roles: "send".to_string(),
                viewer: None,
                secret: None,
                accept_invalid_certs: None,
            },
        )
        .await
        .expect("rename host");

        let saved = Settings::load().expect("reload settings");
        assert_eq!(saved.active_target.as_deref(), Some("new-host"));
    }

    #[tokio::test]
    async fn host_upsert_infers_auth_from_selected_secret() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }
        upsert_secret_auth(
            "api-secret",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "pw",
        )
        .expect("save api key secret");

        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;
        apply_upsert_host(
            &state,
            HostUpsertForm {
                original_name: None,
                name: "with-secret".to_string(),
                auth: String::new(),
                app: "Elasticsearch".to_string(),
                url: "http://localhost:9200".to_string(),
                url_template: None,
                roles: "collect".to_string(),
                viewer: None,
                secret: Some("api-secret".to_string()),
                accept_invalid_certs: None,
            },
        )
        .await
        .expect("save host");

        let saved_hosts = KnownHost::parse_hosts_yml().expect("reload hosts");
        let saved = saved_hosts.get("with-secret").expect("saved host");
        assert_eq!(saved.secret.as_deref(), Some("api-secret"));
        assert!(saved.legacy_apikey.is_none());
    }

    #[tokio::test]
    async fn host_upsert_persists_template_hosts_and_rejects_invalid_templates() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        let state = test_server_state();

        apply_upsert_host(
            &state,
            HostUpsertForm {
                original_name: None,
                name: "elastic-cloud".to_string(),
                auth: "none".to_string(),
                app: "Elasticsearch".to_string(),
                url: "https://cloud.elastic.co/api/v1/deployments/{id}/{product}".to_string(),
                url_template: Some("true".to_string()),
                roles: "collect".to_string(),
                viewer: None,
                secret: None,
                accept_invalid_certs: None,
            },
        )
        .await
        .expect("save template host");

        let saved_hosts = KnownHost::parse_hosts_yml().expect("reload hosts");
        let saved = saved_hosts.get("elastic-cloud").expect("saved template host");
        assert!(saved.url.is_none(), "template host should not persist a concrete url");
        assert_eq!(
            saved.url_template.as_deref(),
            Some("https://cloud.elastic.co/api/v1/deployments/{id}/{product}")
        );

        let err = apply_upsert_host(
            &state,
            HostUpsertForm {
                original_name: None,
                name: "bad-template".to_string(),
                auth: "none".to_string(),
                app: "Elasticsearch".to_string(),
                url: "https://cloud.elastic.co/api/v1/deployments/{unsupported}".to_string(),
                url_template: Some("true".to_string()),
                roles: "collect".to_string(),
                viewer: None,
                secret: None,
                accept_invalid_certs: None,
            },
        )
        .await
        .expect_err("unsupported placeholders should be rejected");
        assert!(err.contains("Unsupported `url_template` placeholder"));
    }

    #[test]
    fn host_draft_initializes_transport_toggle_from_persisted_host_type() {
        let template_row = template::HostsTableRow {
            name: "elastic-cloud".to_string(),
            auth: "none".to_string(),
            app: "Elasticsearch".to_string(),
            url: "https://cloud.elastic.co/api/v1/deployments/{id}/{product}".to_string(),
            url_template: true,
            roles: "collect".to_string(),
            viewer: String::new(),
            accept_invalid_certs: false,
            cloud_id: String::new(),
            secret: String::new(),
        };
        let concrete_row = template::HostsTableRow {
            url_template: false,
            url: "https://prod-es:9200".to_string(),
            ..template_row.clone()
        };

        let template_draft = host_draft(&template_row, Some("elastic-cloud"));
        let concrete_draft = host_draft(&concrete_row, Some("prod-es"));

        assert!(template_draft.url_template);
        assert!(!concrete_draft.url_template);
    }

    #[test]
    fn rendered_host_rows_use_switch_markup_and_distinguish_template_hosts() {
        let template_row = template::HostsTableRow {
            name: "elastic-cloud".to_string(),
            auth: "none".to_string(),
            app: "Elasticsearch".to_string(),
            url: "https://cloud.elastic.co/api/v1/deployments/{id}/{product}".to_string(),
            url_template: true,
            roles: "collect".to_string(),
            viewer: String::new(),
            accept_invalid_certs: false,
            cloud_id: String::new(),
            secret: String::new(),
        };
        let concrete_row = template::HostsTableRow {
            name: "prod-es".to_string(),
            url: "https://prod-es:9200".to_string(),
            url_template: false,
            ..template_row.clone()
        };

        let editing_html = render_host_row(1, &template_row, &[], false, true, true);
        assert!(editing_html.contains("host-url-template-toggle-1"));
        assert!(editing_html.contains("class=\"switch\""));
        assert!(editing_html.contains("Template URL"));
        assert!(editing_html.contains("Concrete URL"));
        assert!(editing_html.contains(r#"<option value="Unknown""#));

        let template_readonly_html = render_host_row(1, &template_row, &[], false, false, true);
        let concrete_readonly_html = render_host_row(2, &concrete_row, &[], false, false, true);
        assert!(template_readonly_html.contains("Template URL:"));
        assert!(concrete_readonly_html.contains("URL:"));
        assert!(!concrete_readonly_html.contains("Template URL:"));
    }

    #[tokio::test]
    async fn secret_cancel_after_lock_timeout_uses_draft_without_keystore_read() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        upsert_secret_auth(
            "existing-secret",
            SecretAuth::Basic {
                username: "elastic".to_string(),
                password: "super-secret-password".to_string(),
            },
            "pw",
        )
        .expect("store secret");

        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;
        state.set_keystore_locked("test timeout").await;

        let signals = SecretsUiSignals {
            secrets: TableUiSignals {
                rows: [(
                    "1".to_string(),
                    TableRowSignals {
                        draft: Some(
                            json!({
                                "original_secret_id": "existing-secret",
                                "secret_id": "renamed-secret",
                                "authtype": "Basic",
                                "username": "elastic"
                            })
                            .as_object()
                            .cloned()
                            .expect("draft object"),
                        ),
                    },
                )]
                .into_iter()
                .collect(),
            },
        };

        let response = secret_action(
            axum::extract::State(state),
            Path(("cancel".to_string(), "1".to_string())),
            Some(ReadSignals(signals)),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("response body");
        let body = String::from_utf8(body.to_vec()).expect("utf8 body");
        assert!(body.contains("secrets-table-row-1"));
        assert!(body.contains("existing-secret"));
        assert!(!body.contains("Keystore password is not available"));
    }

    #[tokio::test]
    async fn persisted_secret_row_delete_requires_secret_id_endpoint() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        upsert_secret_auth(
            "existing-secret",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "pw",
        )
        .expect("store secret");

        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = secret_action(
            axum::extract::State(state),
            Path(("delete".to_string(), "1".to_string())),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("response body");
        let body = String::from_utf8(body.to_vec()).expect("utf8 body");
        assert!(body.contains("Persisted secret deletion must use the secret_id endpoint."));
    }

    #[tokio::test]
    async fn delete_secret_by_id_removes_matching_secret() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        upsert_secret_auth(
            "existing-secret",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "pw",
        )
        .expect("store secret");

        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = delete_secret_by_id(axum::extract::State(state), Path("existing-secret".to_string())).await;

        assert_eq!(response.status(), StatusCode::OK);
        let (_, ids) = read_secret_rows("pw").expect("read secret rows");
        assert!(!ids.iter().any(|secret_id| secret_id == "existing-secret"));
    }

    #[tokio::test]
    async fn delete_secret_by_id_returns_reference_error_patch_when_secret_is_in_use() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        upsert_secret_auth(
            "servermore",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "pw",
        )
        .expect("store secret");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "servermore".to_string(),
            KnownHostBuilder::new(Url::parse("https://servermore.example:9200").expect("url"))
                .product(Product::Elasticsearch)
                .roles(vec![HostRole::Send])
                .secret(Some("servermore".to_string()))
                .build()
                .expect("build host"),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = delete_secret_by_id(axum::extract::State(state), Path("servermore".to_string())).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("response body");
        let body = String::from_utf8(body.to_vec()).expect("utf8 body");
        assert!(body.contains("datastar-patch-elements"));
        assert!(body.contains("selector #secrets-error-modal"));
        assert!(body.contains("id=\"secrets-error-modal\""));
        assert!(body.contains("class=\"modal\""));
        assert!(body.contains("Secret action failed"));
        assert!(body.contains("modal.outerHTML = `<div id='secrets-error-modal'></div>`"));
        assert!(body.contains("Cannot remove secret 'servermore' because it is still referenced by hosts: servermore"));
    }

    #[test]
    fn paired_send_and_view_hosts_render_as_diagnostic_cluster_rows() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "collector".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://collector:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        hosts.insert(
            "prod".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("https://prod-es:9200").expect("url"),
                vec![HostRole::Send],
                Some("prod-kb".to_string()),
                true,
                Some("diag-secret".to_string()),
                None,
            ),
        );
        hosts.insert(
            "prod-kb".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Kibana,
                Url::parse("https://prod-kb:5601").expect("url"),
                vec![HostRole::View],
                None,
                true,
                Some("diag-secret".to_string()),
                None,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let (host_rows, cluster_rows) = read_host_and_cluster_rows();
        assert_eq!(host_rows.len(), 1);
        assert_eq!(host_rows[0].name, "collector");
        assert_eq!(cluster_rows.len(), 1);
        assert_eq!(cluster_rows[0].name, "prod");
        assert_eq!(cluster_rows[0].auth, "secret");
        assert_eq!(cluster_rows[0].secret, "diag-secret");
        assert!(cluster_rows[0].accept_invalid_certs);
        assert_eq!(cluster_rows[0].elasticsearch_url, "https://prod-es:9200/");
        assert_eq!(cluster_rows[0].kibana_url, "https://prod-kb:5601/");
    }

    #[tokio::test]
    async fn renaming_active_cluster_updates_both_hosts_and_settings_target() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "old-cluster".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://old-es:9200").expect("url"),
                vec![HostRole::Send],
                Some("old-cluster-kb".to_string()),
                false,
            ),
        );
        hosts.insert(
            "old-cluster-kb".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://old-kb:5601").expect("url"),
                vec![HostRole::View],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        Settings {
            active_target: Some("old-cluster".to_string()),
            kibana_url: None,
        }
        .save()
        .expect("save settings");

        let state = test_server_state();
        apply_upsert_cluster(
            &state,
            ClusterUpsertForm {
                original_name: Some("old-cluster".to_string()),
                name: "new-cluster".to_string(),
                auth: "none".to_string(),
                secret: None,
                accept_invalid_certs: None,
                elasticsearch_url: "http://new-es:9200".to_string(),
                kibana_url: "http://new-kb:5601".to_string(),
            },
        )
        .await
        .expect("rename cluster");

        let saved_hosts = KnownHost::parse_hosts_yml().expect("reload hosts");
        assert!(saved_hosts.contains_key("new-cluster"));
        assert!(saved_hosts.contains_key("new-cluster-kb"));
        assert!(!saved_hosts.contains_key("old-cluster"));
        assert!(!saved_hosts.contains_key("old-cluster-kb"));
        assert_eq!(saved_hosts["new-cluster"].viewer(), Some("new-cluster-kb"));
        assert!(saved_hosts["new-cluster"].has_role(HostRole::Send));
        assert!(saved_hosts["new-cluster-kb"].has_role(HostRole::View));

        let saved_settings = Settings::load().expect("reload settings");
        assert_eq!(saved_settings.active_target.as_deref(), Some("new-cluster"));
    }

    #[test]
    fn rendered_secret_rows_expose_metadata_but_not_persisted_secret_values() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        authenticate("pw").expect("create keystore");
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }
        upsert_secret_auth(
            "api-secret",
            SecretAuth::ApiKey {
                apikey: "super-secret-api-key".to_string(),
            },
            "pw",
        )
        .expect("save api key secret");
        upsert_secret_auth(
            "basic-secret",
            SecretAuth::Basic {
                username: "elastic".to_string(),
                password: "super-secret-password".to_string(),
            },
            "pw",
        )
        .expect("save basic secret");

        let (rows, _) = read_secret_rows("pw").expect("read secret rows");
        let html = rows
            .iter()
            .enumerate()
            .map(|(idx, row)| render_secret_row(idx + 1, row, false, true, false))
            .collect::<String>();

        assert!(html.contains("api-secret"));
        assert!(html.contains("basic-secret"));
        assert!(html.contains("elastic:"));
        assert!(html.contains("encodeURIComponent(evt.currentTarget.closest('tr').dataset.secretLabel)"));
        assert!(!html.contains("super-secret-api-key"));
        assert!(!html.contains("super-secret-password"));
    }

    #[test]
    fn rendered_secrets_panel_includes_secret_error_modal_placeholder() {
        let html = render_secrets_panel(&[], false).expect("render secrets panel");

        assert!(html.contains("id=\"secrets-error-modal\""));
    }
}
