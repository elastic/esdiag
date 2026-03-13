use super::{ServerState, get_theme_dark, template};
use crate::data::{
    ElasticCloud, HostRole, KnownHost, Product, SecretAuth, Settings, keystore_exists,
    list_secret_names, remove_secret, resolve_secret_auth, upsert_secret_auth,
};
use askama::Template;
use axum::{
    extract::{Form, Json, Path, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
};
use datastar::{
    axum::ReadSignals, consts::ElementPatchMode, patch_elements::PatchElements,
    patch_signals::PatchSignals,
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
    pub roles: String,
    pub viewer: Option<String>,
    pub secret: Option<String>,
    pub apikey: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
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
    pub roles: String,
    #[serde(default)]
    pub viewer: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub apikey: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
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
    roles: String,
    #[serde(default)]
    viewer: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    apikey: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
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
    let (keystore_locked, keystore_lock_time) = state.keystore_status_for(&user_email).await;
    let can_use_keystore =
        cfg!(feature = "keystore") && state.runtime_mode_policy.allows_local_artifacts();
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);

    let (hosts, clusters) = read_host_and_cluster_rows();
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match current_keystore_password(&state, &user_email)
            .await
            .and_then(|password| read_secret_rows(&password))
        {
            Ok((rows, ids)) => {
                state.touch_keystore_session_for(&user_email).await;
                (rows, ids)
            }
            Err(err) => {
                log::warn!("Failed to list keystore secrets for /settings: {}", err);
                (Vec::new(), Vec::new())
            }
        }
    };
    let hosts_panel_html =
        render_hosts_panel(&hosts, &secret_ids, keystore_locked).unwrap_or_else(|err| {
            format!("<panel id=\"hosts-table-panel\"><div>Error: {err}</div></panel>")
        });
    let secrets_panel_html =
        render_secrets_panel(&secrets, keystore_locked).unwrap_or_else(|err| {
            format!("<panel id=\"secrets-table-panel\"><div>Error: {err}</div></panel>")
        });
    let clusters_panel_html = render_clusters_panel(&clusters, &secret_ids, keystore_locked)
        .unwrap_or_else(|err| {
            format!("<panel id=\"clusters-table-panel\"><div>Error: {err}</div></panel>")
        });

    let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
    let exporter = state.exporter.read().await.clone();
    let preferred_target = if state.runtime_mode_policy.allows_local_artifacts() {
        Settings::load()
            .ok()
            .and_then(|settings| settings.active_target)
    } else {
        None
    };
    let (output_options, selected_output, exporter_label) = template::build_footer_output_context(
        &hosts_by_name,
        &exporter,
        preferred_target.as_deref(),
    );
    let active_output_secure =
        template::active_output_requires_keystore(&hosts_by_name, &selected_output, &exporter);
    let page = template::HostsPage {
        auth_header,
        debug: log::max_level() >= log::LevelFilter::Debug,
        desktop: cfg!(feature = "desktop"),
        can_configure_output: state.runtime_mode_policy.allows_exporter_updates(),
        output_options,
        selected_output,
        exporter_label,
        active_output_secure,
        kibana_url: state.kibana_url.read().await.clone(),
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark: get_theme_dark(&headers),
        runtime_mode: state.runtime_mode.to_string(),
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
    headers: HeaderMap,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<HostsUiSignals>>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);
    let editing_action = matches!(action.as_str(), "create" | "read" | "update");

    if editing_action && !state.is_keystore_unlocked_for(&user).await {
        log::warn!(
            "Rejected host action '{}' for row '{}' because keystore is locked",
            action,
            id
        );
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before editing hosts.",
        )
            .into_response();
    }

    if action == "create" {
        if id != "new" {
            log::warn!("Rejected host create with unexpected id '{}'", id);
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }

        let hosts = read_host_rows();
        let secret_ids = load_secret_ids(&state, &user).await;
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
            log::warn!(
                "Rejected host action '{}' with non-numeric id '{}'",
                action,
                id
            );
            return (StatusCode::BAD_REQUEST, "id must be numeric or 'new'").into_response();
        }
    };
    let hosts = read_host_rows();

    match action.as_str() {
        "read" => {
            let Some(host) = host_row_by_id(&hosts, row_id).cloned() else {
                log::warn!("Host read for row {} failed: row not found", row_id);
                return (StatusCode::NOT_FOUND, "host row not found").into_response();
            };
            let secret_ids = load_secret_ids(&state, &user).await;
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
                log::warn!(
                    "Host cancel for transient row {} removed patched row",
                    row_id
                );
                return clear_and_remove_row("hosts", &host_row_selector(row_id), row_id);
            };
            let secret_ids = load_secret_ids(&state, &user).await;
            sse_response(vec![
                clear_row_signal_patch("hosts", row_id)
                    .as_datastar_event()
                    .to_string(),
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
                log::warn!("Host update for row {} failed: {}", row_id, message);
                return sse_response(vec![patch_host_error_event(&message)]);
            };
            let host_name = draft.name.clone();
            match apply_upsert_host(&state, draft, &user).await {
                Ok(_) => patch_host_row_saved_response(&state, row_id, &host_name, &user).await,
                Err(err) => {
                    log::warn!("Host update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_host_error_event(&err)])
                }
            }
        }
        "delete" => delete_host_row(&hosts, row_id).await,
        _ => {
            log::warn!(
                "Rejected unsupported host action '{}' for row {}",
                action,
                row_id
            );
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
    headers: HeaderMap,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<ClustersUiSignals>>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);
    let editing_action = matches!(action.as_str(), "create" | "read" | "update");

    if editing_action && !state.is_keystore_unlocked_for(&user).await {
        log::warn!(
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
            log::warn!("Rejected cluster create with unexpected id '{}'", id);
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }

        let clusters = read_host_and_cluster_rows().1;
        let secret_ids = load_secret_ids(&state, &user).await;
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
            log::warn!(
                "Rejected cluster action '{}' with non-numeric id '{}'",
                action,
                id
            );
            return (StatusCode::BAD_REQUEST, "id must be numeric or 'new'").into_response();
        }
    };
    let clusters = read_host_and_cluster_rows().1;

    match action.as_str() {
        "read" => {
            let Some(cluster) = cluster_row_by_id(&clusters, row_id).cloned() else {
                log::warn!("Cluster read for row {} failed: row not found", row_id);
                return (StatusCode::NOT_FOUND, "cluster row not found").into_response();
            };
            let secret_ids = load_secret_ids(&state, &user).await;
            sse_response(vec![
                row_signal_patch(
                    "clusters",
                    row_id,
                    &cluster_draft(&cluster, Some(&cluster.name)),
                )
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
                log::warn!(
                    "Cluster cancel for transient row {} removed patched row",
                    row_id
                );
                return clear_and_remove_row("clusters", &cluster_row_selector(row_id), row_id);
            };
            let secret_ids = load_secret_ids(&state, &user).await;
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
                log::warn!("Cluster update for row {} failed: {}", row_id, message);
                return sse_response(vec![patch_cluster_error_event(&message)]);
            };
            match apply_upsert_cluster(&state, draft, &user).await {
                Ok(_) => {
                    patch_hosts_and_secrets_with_clear_response(
                        &state,
                        Some(("clusters", row_id)),
                        &user,
                    )
                    .await
                }
                Err(err) => {
                    log::warn!("Cluster update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_cluster_error_event(&err)])
                }
            }
        }
        "delete" => delete_cluster_row(&state, &clusters, row_id, &user).await,
        _ => (
            StatusCode::BAD_REQUEST,
            "unsupported action; supported: create, read, cancel, update, delete",
        )
            .into_response(),
    }
}

pub async fn secret_action(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Path((action, id)): Path<(String, String)>,
    signals: Option<ReadSignals<SecretsUiSignals>>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    let signal_state = signals.as_ref().map(|ReadSignals(signals)| signals);

    if action == "create" {
        if id != "new" {
            return (StatusCode::BAD_REQUEST, "create expects id 'new'").into_response();
        }
        if !state.is_keystore_unlocked_for(&user).await {
            return (
                StatusCode::PRECONDITION_FAILED,
                "Unlock keystore before editing secrets.",
            )
                .into_response();
        }

        let secrets = current_keystore_password(&state, &user)
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
    let (secrets, _) = match current_keystore_password(&state, &user)
        .await
        .and_then(|password| read_secret_rows(&password))
    {
        Ok(result) => result,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    match action.as_str() {
        "read" => {
            let Some(secret) = secret_row_by_id(&secrets, row_id).cloned() else {
                return (StatusCode::NOT_FOUND, "secret row not found").into_response();
            };
            sse_response(vec![
                row_signal_patch(
                    "secrets",
                    row_id,
                    &secret_draft(&secret, Some(&secret.secret_id)),
                )
                .as_datastar_event()
                .to_string(),
                patch_row_event(
                    &secret_row_selector(row_id),
                    render_secret_row(row_id, &secret, true, true, false),
                )
                .to_string(),
            ])
        }
        "cancel" => {
            let Some(secret) = secret_row_by_id(&secrets, row_id).cloned() else {
                return clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id);
            };
            sse_response(vec![
                clear_row_signal_patch("secrets", row_id)
                    .as_datastar_event()
                    .to_string(),
                patch_row_event(
                    &secret_row_selector(row_id),
                    render_secret_row(row_id, &secret, false, true, false),
                )
                .to_string(),
            ])
        }
        "update" => {
            let Some(draft) = secret_draft_from_signals(signal_state, row_id) else {
                return (StatusCode::BAD_REQUEST, "missing secret row draft signals")
                    .into_response();
            };
            match apply_upsert_secret(&state, draft, &user).await {
                Ok(_) => {
                    patch_hosts_and_secrets_with_clear_response(
                        &state,
                        Some(("secrets", row_id)),
                        &user,
                    )
                    .await
                }
                Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
            }
        }
        "delete" => delete_secret_row(&state, &secrets, row_id, &user).await,
        _ => (
            StatusCode::BAD_REQUEST,
            "unsupported action; supported: create, read, cancel, update, delete",
        )
            .into_response(),
    }
}

pub async fn upsert_host(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<HostUpsertForm>,
) -> Response {
    match apply_upsert_host(&state, form, "Anonymous").await {
        Ok(_) => patch_hosts_panel_response(&state).await,
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn create_host(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<HostRecordPayload>,
) -> Response {
    let form = HostUpsertForm {
        original_name: None,
        name: payload.name,
        auth: payload.auth,
        app: payload.app,
        url: payload.url,
        roles: payload.roles,
        viewer: payload.viewer,
        secret: payload.secret,
        apikey: payload.apikey,
        username: payload.username,
        password: payload.password,
        accept_invalid_certs: payload.accept_invalid_certs.then_some("true".to_string()),
    };
    match apply_upsert_host(&state, form, "Anonymous").await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn update_host(
    State(state): State<Arc<ServerState>>,
    Json(payload): Json<HostRecordPayload>,
) -> Response {
    let form = HostUpsertForm {
        original_name: payload.original_name,
        name: payload.name,
        auth: payload.auth,
        app: payload.app,
        url: payload.url,
        roles: payload.roles,
        viewer: payload.viewer,
        secret: payload.secret,
        apikey: payload.apikey,
        username: payload.username,
        password: payload.password,
        accept_invalid_certs: payload.accept_invalid_certs.then_some("true".to_string()),
    };
    match apply_upsert_host(&state, form, "Anonymous").await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_host(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<HostDeleteForm>,
) -> Response {
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

pub async fn upsert_secret(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Form(form): Form<SecretUpsertForm>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    match apply_upsert_secret(&state, form, &user).await {
        Ok(_) => patch_hosts_and_secrets_response(&state, &user).await,
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_secret(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Form(form): Form<SecretDeleteForm>,
) -> Response {
    let user = resolve_request_user(&state, &headers);
    if !state.is_keystore_unlocked_for(&user).await {
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before deleting secrets.",
        )
            .into_response();
    }

    let password = match current_keystore_password(&state, &user).await {
        Ok(password) => password,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    match remove_secret(form.secret_id.trim(), None, &password) {
        Ok(_) => {
            state.touch_keystore_session_for(&user).await;
            patch_hosts_and_secrets_response(&state, &user).await
        }
        Err(err) => (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    }
}

fn read_host_and_cluster_rows() -> (
    Vec<template::HostsTableRow>,
    Vec<template::DiagnosticClusterTableRow>,
) {
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
        url: host.get_url().to_string(),
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
        apikey: String::new(),
        username: String::new(),
        password: String::new(),
    };
    match host {
        KnownHost::ApiKey {
            cloud_id,
            secret,
            apikey,
            ..
        } => {
            row.auth = "apikey".to_string();
            row.cloud_id = cloud_id.map(|v| v.to_string()).unwrap_or_default();
            row.secret = secret.unwrap_or_default();
            row.apikey = apikey.unwrap_or_default();
        }
        KnownHost::Basic {
            secret,
            username,
            password,
            ..
        } => {
            row.auth = "basic".to_string();
            row.secret = secret.unwrap_or_default();
            row.username = username.unwrap_or_default();
            row.password = password.unwrap_or_default();
        }
        KnownHost::NoAuth { .. } => {}
    }
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
        elasticsearch_url: host.get_url().to_string(),
        kibana_url: kibana_host.get_url().to_string(),
    })
}

fn host_auth_value(host: &KnownHost) -> &'static str {
    match host {
        KnownHost::ApiKey { .. } => "apikey",
        KnownHost::Basic { .. } => "basic",
        KnownHost::NoAuth { .. } => "none",
    }
}

fn host_secret_value(host: &KnownHost) -> String {
    match host {
        KnownHost::ApiKey { secret, .. } | KnownHost::Basic { secret, .. } => {
            secret.clone().unwrap_or_default()
        }
        KnownHost::NoAuth { .. } => String::new(),
    }
}

fn host_has_plaintext_auth(host: &KnownHost) -> bool {
    match host {
        KnownHost::ApiKey { apikey, .. } => apikey.is_some(),
        KnownHost::Basic {
            username, password, ..
        } => username.is_some() || password.is_some(),
        KnownHost::NoAuth { .. } => false,
    }
}

fn read_secret_rows(
    keystore_password: &str,
) -> Result<(Vec<template::SecretTableRow>, Vec<String>), String> {
    let secret_ids = list_secret_names(keystore_password).map_err(to_message)?;
    let mut rows = Vec::new();

    for secret_id in &secret_ids {
        let auth = resolve_secret_auth(secret_id, keystore_password).map_err(to_message)?;
        let (auth_type, username) = match auth {
            Some(SecretAuth::ApiKey { .. }) => ("ApiKey", String::new()),
            Some(SecretAuth::Basic { username, .. }) => ("Basic", username),
            None => ("Unknown", String::new()),
        };
        rows.push(template::SecretTableRow {
            secret_id: secret_id.clone(),
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
            .map(|(idx, host)| {
                render_host_row(idx + 1, host, secret_ids, keystore_locked, false, true)
            })
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
            .map(|(idx, cluster)| {
                render_cluster_row(idx + 1, cluster, secret_ids, keystore_locked, false, true)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
    .render()
    .map_err(to_message)
}

fn render_secrets_panel(
    secrets: &[template::SecretTableRow],
    keystore_locked: bool,
) -> Result<String, String> {
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
    user: &str,
) -> (
    Vec<template::HostsTableRow>,
    Vec<template::DiagnosticClusterTableRow>,
    Vec<template::SecretTableRow>,
    Vec<String>,
    bool,
) {
    let (hosts, clusters) = read_host_and_cluster_rows();
    let keystore_locked = state.keystore_status_for(user).await.0;
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match current_keystore_password(state, user)
            .await
            .and_then(|password| read_secret_rows(&password))
        {
            Ok((rows, ids)) => {
                state.touch_keystore_session_for(user).await;
                (rows, ids)
            }
            Err(err) => {
                log::warn!(
                    "Failed to list keystore secrets for /settings patch: {}",
                    err
                );
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
        let message = message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
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
        let message = message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
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

fn sse_response(events: Vec<String>) -> Response {
    ([(CONTENT_TYPE, "text/event-stream")], events.join("\n\n")).into_response()
}

async fn patch_hosts_panel_response(state: &Arc<ServerState>) -> Response {
    let (hosts, _clusters, _secrets, secret_ids, keystore_locked) =
        load_table_panel_data(state, "Anonymous").await;
    match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => sse_response(vec![patch_panel_event("#hosts-table-panel", html)]),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

async fn patch_hosts_and_secrets_response(state: &Arc<ServerState>, user: &str) -> Response {
    let (hosts, clusters, secrets, secret_ids, keystore_locked) =
        load_table_panel_data(state, user).await;
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
    user: &str,
) -> Response {
    let (hosts, clusters, secrets, secret_ids, keystore_locked) =
        load_table_panel_data(state, user).await;
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
        events.push(
            clear_row_signal_patch(panel, row_id)
                .as_datastar_event()
                .to_string(),
        );
    }
    events.push(patch_panel_event("#hosts-table-panel", hosts_html));
    events.push(patch_panel_event("#secrets-table-panel", secrets_html));
    events.push(patch_panel_event("#clusters-table-panel", clusters_html));
    sse_response(events)
}

async fn apply_upsert_host(
    state: &Arc<ServerState>,
    form: HostUpsertForm,
    user: &str,
) -> Result<(), String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("Host name is required.".to_string());
    }

    let url = Url::parse(form.url.trim()).map_err(|err| format!("Invalid URL: {err}"))?;
    let app = parse_product(form.app.trim())?;
    let roles = parse_roles(&form.roles)?;
    let viewer = to_opt(form.viewer);
    let secret = to_opt(form.secret);
    let apikey = to_opt(form.apikey);
    let username = to_opt(form.username);
    let password = to_opt(form.password);
    let accept_invalid_certs = form.accept_invalid_certs.is_some();
    let auth = infer_auth_from_secret_selection(state, user, &secret, form.auth.trim()).await?;

    let cloud_id = ElasticCloud::try_from(&url).ok();
    let host = match auth.as_str() {
        "none" => KnownHost::NoAuth {
            app,
            roles,
            viewer,
            url,
        },
        "apikey" => KnownHost::ApiKey {
            accept_invalid_certs,
            apikey,
            app,
            cloud_id,
            roles,
            secret,
            viewer,
            url,
        },
        "basic" => KnownHost::Basic {
            accept_invalid_certs,
            app,
            password,
            roles,
            secret,
            viewer,
            url,
            username,
        },
        _ => return Err("Auth type must be one of: none, apikey, basic.".to_string()),
    };

    let mut hosts: BTreeMap<String, KnownHost> =
        KnownHost::parse_hosts_yml().map_err(to_message)?;
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

async fn apply_upsert_cluster(
    state: &Arc<ServerState>,
    form: ClusterUpsertForm,
    user: &str,
) -> Result<(), String> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err("Diagnostic cluster name is required.".to_string());
    }

    let elasticsearch_url = Url::parse(form.elasticsearch_url.trim())
        .map_err(|err| format!("Invalid Elasticsearch URL: {err}"))?;
    let kibana_url =
        Url::parse(form.kibana_url.trim()).map_err(|err| format!("Invalid Kibana URL: {err}"))?;
    let secret = to_opt(form.secret);
    let accept_invalid_certs = form.accept_invalid_certs.is_some();
    let auth = infer_auth_from_secret_selection(state, user, &secret, form.auth.trim()).await?;

    let kibana_name = format!("{name}-kb");
    let elasticsearch_host = match auth.as_str() {
        "none" => KnownHost::NoAuth {
            app: Product::Elasticsearch,
            roles: vec![HostRole::Send],
            viewer: Some(kibana_name.clone()),
            url: elasticsearch_url,
        },
        "apikey" => KnownHost::ApiKey {
            accept_invalid_certs,
            apikey: None,
            app: Product::Elasticsearch,
            cloud_id: ElasticCloud::try_from(&elasticsearch_url).ok(),
            roles: vec![HostRole::Send],
            secret: secret.clone(),
            viewer: Some(kibana_name.clone()),
            url: elasticsearch_url,
        },
        "basic" => KnownHost::Basic {
            accept_invalid_certs,
            app: Product::Elasticsearch,
            password: None,
            roles: vec![HostRole::Send],
            secret: secret.clone(),
            viewer: Some(kibana_name.clone()),
            url: elasticsearch_url,
            username: None,
        },
        _ => return Err("Auth type must be one of: none, apikey, basic.".to_string()),
    };
    let kibana_host = match auth.as_str() {
        "none" => KnownHost::NoAuth {
            app: Product::Kibana,
            roles: vec![HostRole::View],
            viewer: None,
            url: kibana_url,
        },
        "apikey" => KnownHost::ApiKey {
            accept_invalid_certs,
            apikey: None,
            app: Product::Kibana,
            cloud_id: ElasticCloud::try_from(&kibana_url).ok(),
            roles: vec![HostRole::View],
            secret: secret.clone(),
            viewer: None,
            url: kibana_url,
        },
        "basic" => KnownHost::Basic {
            accept_invalid_certs,
            app: Product::Kibana,
            password: None,
            roles: vec![HostRole::View],
            secret: secret.clone(),
            viewer: None,
            url: kibana_url,
            username: None,
        },
        _ => unreachable!(),
    };

    let mut hosts: BTreeMap<String, KnownHost> =
        KnownHost::parse_hosts_yml().map_err(to_message)?;
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

async fn apply_upsert_secret(
    state: &Arc<ServerState>,
    form: SecretUpsertForm,
    user: &str,
) -> Result<(), String> {
    if !state.is_keystore_unlocked_for(user).await {
        return Err("Unlock keystore before editing secrets.".to_string());
    }

    let secret_id = form.secret_id.trim();
    if secret_id.is_empty() {
        return Err("secret_id is required.".to_string());
    }
    let password = current_keystore_password(state, user).await?;

    let auth = match form.auth_type.trim().to_ascii_lowercase().as_str() {
        "apikey" => {
            let apikey = to_opt(form.apikey).ok_or("apikey is required for ApiKey type.")?;
            SecretAuth::ApiKey { apikey }
        }
        "basic" => {
            let username = to_opt(form.username).ok_or("username is required for Basic type.")?;
            let password_value =
                to_opt(form.password).ok_or("password is required for Basic type.")?;
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
    state.touch_keystore_session_for(user).await;
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

fn host_row_by_id(
    hosts: &[template::HostsTableRow],
    row_id: usize,
) -> Option<&template::HostsTableRow> {
    row_id.checked_sub(1).and_then(|idx| hosts.get(idx))
}

fn secret_row_by_id(
    secrets: &[template::SecretTableRow],
    row_id: usize,
) -> Option<&template::SecretTableRow> {
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
        roles: "collect".to_string(),
        viewer: String::new(),
        accept_invalid_certs: false,
        cloud_id: String::new(),
        secret: String::new(),
        apikey: String::new(),
        username: String::new(),
        password: String::new(),
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
        roles: host.roles.clone(),
        viewer: Some(host.viewer.clone()),
        secret: Some(host.secret.clone()),
        apikey: None,
        username: None,
        password: None,
        accept_invalid_certs: host.accept_invalid_certs,
    }
}

fn cluster_draft(
    cluster: &template::DiagnosticClusterTableRow,
    original_name: Option<&str>,
) -> ClusterDraftSignal {
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

fn secret_draft(
    secret: &template::SecretTableRow,
    original_secret_id: Option<&str>,
) -> SecretDraftSignal {
    SecretDraftSignal {
        original_secret_id: original_secret_id.map(str::to_string),
        secret_id: secret.secret_id.clone(),
        auth_type: secret.auth_type.clone(),
        apikey: None,
        username: (!secret.username.is_empty()).then(|| secret.username.clone()),
        password: None,
    }
}

fn host_draft_from_signals(
    signals: Option<&HostsUiSignals>,
    row_id: usize,
) -> Option<HostUpsertForm> {
    let draft: HostDraftSignal = draft_from_rows(signals.map(|s| &s.hosts), row_id)?;
    Some(HostUpsertForm {
        original_name: draft.original_name,
        name: draft.name,
        auth: draft.auth,
        app: draft.app,
        url: draft.url,
        roles: draft.roles,
        viewer: draft.viewer,
        secret: draft.secret,
        apikey: draft.apikey,
        username: draft.username,
        password: draft.password,
        accept_invalid_certs: draft.accept_invalid_certs.then_some("true".to_string()),
    })
}

fn cluster_draft_from_signals(
    signals: Option<&ClustersUiSignals>,
    row_id: usize,
) -> Option<ClusterUpsertForm> {
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

fn secret_draft_from_signals(
    signals: Option<&SecretsUiSignals>,
    row_id: usize,
) -> Option<SecretUpsertForm> {
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

fn draft_from_rows<T: for<'de> Deserialize<'de>>(
    signals: Option<&TableUiSignals>,
    row_id: usize,
) -> Option<T> {
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

async fn load_secret_ids(state: &Arc<ServerState>, user: &str) -> Vec<String> {
    if !state.is_keystore_unlocked_for(user).await {
        return Vec::new();
    }
    match current_keystore_password(state, user)
        .await
        .and_then(|password| read_secret_rows(&password))
    {
        Ok((_, ids)) => {
            state.touch_keystore_session_for(user).await;
            ids
        }
        Err(err) => {
            log::warn!("Failed to read secret ids for hosts row render: {}", err);
            Vec::new()
        }
    }
}

async fn delete_host_row(hosts: &[template::HostsTableRow], row_id: usize) -> Response {
    let Some(host) = host_row_by_id(hosts, row_id) else {
        log::warn!(
            "Host delete for transient row {} removed patched row",
            row_id
        );
        return clear_and_remove_row("hosts", &host_row_selector(row_id), row_id);
    };

    let mut host_map = KnownHost::parse_hosts_yml().map_err(to_message);
    if let Ok(ref mut host_map) = host_map {
        host_map.remove(host.name.trim());
        return match KnownHost::write_hosts_yml(host_map) {
            Ok(_) => sse_response(vec![
                clear_row_signal_patch("hosts", row_id)
                    .as_datastar_event()
                    .to_string(),
                patch_host_error_event(""),
                PatchElements::new_remove(host_row_selector(row_id))
                    .as_datastar_event()
                    .to_string(),
            ]),
            Err(err) => {
                let message = to_message(err);
                log::warn!("Host delete for row {} failed: {}", row_id, message);
                sse_response(vec![patch_host_error_event(&message)])
            }
        };
    }

    let message = host_map.err().unwrap_or_default();
    log::warn!("Host delete for row {} failed: {}", row_id, message);
    sse_response(vec![patch_host_error_event(&message)])
}

async fn delete_cluster_row(
    state: &Arc<ServerState>,
    clusters: &[template::DiagnosticClusterTableRow],
    row_id: usize,
    user: &str,
) -> Response {
    let Some(cluster) = cluster_row_by_id(clusters, row_id) else {
        log::warn!(
            "Cluster delete for transient row {} removed patched row",
            row_id
        );
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
                    log::warn!("Cluster delete for row {} failed: {}", row_id, message);
                    return sse_response(vec![patch_cluster_error_event(&message)]);
                }
                patch_hosts_and_secrets_with_clear_response(state, Some(("clusters", row_id)), user)
                    .await
            }
            Err(err) => {
                let message = to_message(err);
                log::warn!("Cluster delete for row {} failed: {}", row_id, message);
                sse_response(vec![patch_cluster_error_event(&message)])
            }
        };
    }

    let message = host_map.err().unwrap_or_default();
    log::warn!("Cluster delete for row {} failed: {}", row_id, message);
    sse_response(vec![patch_cluster_error_event(&message)])
}

async fn delete_secret_row(
    state: &Arc<ServerState>,
    secrets: &[template::SecretTableRow],
    row_id: usize,
    user: &str,
) -> Response {
    let Some(secret) = secret_row_by_id(secrets, row_id) else {
        return clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id);
    };
    if !state.is_keystore_unlocked_for(user).await {
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before deleting secrets.",
        )
            .into_response();
    }

    let password = match current_keystore_password(state, user).await {
        Ok(password) => password,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    match remove_secret(secret.secret_id.trim(), None, &password) {
        Ok(_) => {
            state.touch_keystore_session_for(user).await;
            patch_hosts_and_secrets_with_clear_response(state, Some(("secrets", row_id)), user)
                .await
        }
        Err(err) => (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    }
}

fn clear_and_remove_row(panel: &str, selector: &str, row_id: usize) -> Response {
    sse_response(vec![
        clear_row_signal_patch(panel, row_id)
            .as_datastar_event()
            .to_string(),
        PatchElements::new_remove(selector)
            .as_datastar_event()
            .to_string(),
    ])
}

async fn patch_host_row_saved_response(
    state: &Arc<ServerState>,
    row_id: usize,
    host_name: &str,
    user: &str,
) -> Response {
    let hosts = read_host_rows();
    let Some(host) = hosts.iter().find(|host| host.name == host_name).cloned() else {
        let message = format!(
            "Saved host '{}' but could not reload the updated row.",
            host_name
        );
        log::warn!("{}", message);
        return sse_response(vec![patch_host_error_event(&message)]);
    };
    let keystore_locked = state.keystore_status_for(user).await.0;
    sse_response(vec![
        clear_row_signal_patch("hosts", row_id)
            .as_datastar_event()
            .to_string(),
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
    user: &str,
    secret: &Option<String>,
    fallback_auth: &str,
) -> Result<String, String> {
    if let Some(secret_id) = secret {
        if !state.is_keystore_unlocked_for(user).await {
            return Err("Unlock keystore before selecting a secret.".to_string());
        }
        let password = current_keystore_password(state, user).await?;
        let resolved = resolve_secret_auth(secret_id, &password).map_err(to_message)?;
        state.touch_keystore_session_for(user).await;
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

fn resolve_request_user(state: &Arc<ServerState>, headers: &HeaderMap) -> String {
    state
        .resolve_user_email(headers)
        .map(|(_, user)| user)
        .unwrap_or_else(|_| "Anonymous".to_string())
}

async fn current_keystore_password(state: &Arc<ServerState>, user: &str) -> Result<String, String> {
    state
        .keystore_password_for(user)
        .await
        .ok_or_else(|| "Keystore password is not available for the current session.".to_string())
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::{
        ClusterUpsertForm, HostUpsertForm, apply_upsert_cluster, apply_upsert_host, host_draft,
        read_host_and_cluster_rows, read_secret_rows, render_secret_row,
    };
    use crate::{
        data::{
            HostRole, KnownHost, Product, SecretAuth, Settings, authenticate, upsert_secret_auth,
        },
        server::{template, test_server_state},
    };
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
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
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
                roles: "send".to_string(),
                viewer: None,
                secret: None,
                apikey: None,
                username: None,
                password: None,
                accept_invalid_certs: None,
            },
            "Anonymous",
        )
        .await
        .expect("rename host");

        let saved = Settings::load().expect("reload settings");
        assert_eq!(saved.active_target.as_deref(), Some("new-host"));
    }

    #[test]
    fn host_draft_omits_persisted_plaintext_credentials() {
        let draft = host_draft(
            &template::HostsTableRow {
                name: "legacy-host".to_string(),
                auth: "basic".to_string(),
                app: "Elasticsearch".to_string(),
                url: "https://legacy.example:9200/".to_string(),
                roles: "collect".to_string(),
                viewer: String::new(),
                accept_invalid_certs: false,
                cloud_id: String::new(),
                secret: String::new(),
                apikey: "legacy-api-key".to_string(),
                username: "legacy-user".to_string(),
                password: "legacy-password".to_string(),
            },
            Some("legacy-host"),
        );

        assert_eq!(draft.apikey, None);
        assert_eq!(draft.username, None);
        assert_eq!(draft.password, None);
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
                roles: "collect".to_string(),
                viewer: None,
                secret: Some("api-secret".to_string()),
                apikey: None,
                username: None,
                password: None,
                accept_invalid_certs: None,
            },
            "Anonymous",
        )
        .await
        .expect("save host");

        let saved_hosts = KnownHost::parse_hosts_yml().expect("reload hosts");
        match saved_hosts.get("with-secret").expect("saved host") {
            KnownHost::ApiKey { secret, .. } => {
                assert_eq!(secret.as_deref(), Some("api-secret"));
            }
            _ => panic!("expected ApiKey host"),
        }
    }

    #[test]
    fn paired_send_and_view_hosts_render_as_diagnostic_cluster_rows() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "collector".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect],
                viewer: None,
                url: Url::parse("http://collector:9200").expect("url"),
            },
        );
        hosts.insert(
            "prod".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: true,
                apikey: None,
                app: Product::Elasticsearch,
                cloud_id: None,
                roles: vec![HostRole::Send],
                secret: Some("diag-secret".to_string()),
                viewer: Some("prod-kb".to_string()),
                url: Url::parse("https://prod-es:9200").expect("url"),
            },
        );
        hosts.insert(
            "prod-kb".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: true,
                apikey: None,
                app: Product::Kibana,
                cloud_id: None,
                roles: vec![HostRole::View],
                secret: Some("diag-secret".to_string()),
                viewer: None,
                url: Url::parse("https://prod-kb:5601").expect("url"),
            },
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let (host_rows, cluster_rows) = read_host_and_cluster_rows();
        assert_eq!(host_rows.len(), 1);
        assert_eq!(host_rows[0].name, "collector");
        assert_eq!(cluster_rows.len(), 1);
        assert_eq!(cluster_rows[0].name, "prod");
        assert_eq!(cluster_rows[0].auth, "apikey");
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
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: Some("old-cluster-kb".to_string()),
                url: Url::parse("http://old-es:9200").expect("url"),
            },
        );
        hosts.insert(
            "old-cluster-kb".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::View],
                viewer: None,
                url: Url::parse("http://old-kb:5601").expect("url"),
            },
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
            "Anonymous",
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
        assert!(!html.contains("super-secret-api-key"));
        assert!(!html.contains("super-secret-password"));
    }
}
