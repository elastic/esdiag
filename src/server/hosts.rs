use super::{ServerState, get_theme_dark, template};
use crate::data::{
    ElasticCloud, HostRole, KnownHost, Product, SecretAuth, Settings, get_password_from_env,
    list_secret_names, remove_secret, resolve_secret_auth, upsert_secret_auth,
};
use askama::Template;
use axum::{
    extract::{Form, Json, Path, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
};
use datastar::{
    axum::ReadSignals,
    consts::ElementPatchMode,
    patch_elements::PatchElements,
    patch_signals::PatchSignals,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{collections::{BTreeMap, HashMap}, str::FromStr, sync::Arc};
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
    let (keystore_locked, keystore_lock_time) = state.keystore_status().await;
    let can_use_keystore =
        cfg!(feature = "keystore") && state.runtime_mode_policy.allows_local_artifacts();

    let hosts = read_host_rows();
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match read_secret_rows() {
            Ok((rows, ids)) => {
                state.touch_keystore_session().await;
                (rows, ids)
            }
            Err(err) => {
                log::warn!("Failed to list keystore secrets for /hosts: {}", err);
                (Vec::new(), Vec::new())
            }
        }
    };
    let hosts_panel_html = render_hosts_panel(&hosts, &secret_ids, keystore_locked).unwrap_or_else(|err| {
        format!("<panel id=\"hosts-table-panel\"><div>Error: {err}</div></panel>")
    });
    let secrets_panel_html =
        render_secrets_panel(&secrets, keystore_locked).unwrap_or_else(|err| {
            format!("<panel id=\"secrets-table-panel\"><div>Error: {err}</div></panel>")
        });

    let send_hosts = KnownHost::list_by_role(HostRole::Send).unwrap_or_default();
    let exporter = state.exporter.read().await.clone();
    let preferred_target = if state.runtime_mode_policy.allows_local_artifacts() {
        Settings::load().ok().and_then(|settings| settings.active_target)
    } else {
        None
    };
    let (output_options, selected_output, exporter_label) =
        template::build_footer_output_context(&send_hosts, &exporter, preferred_target.as_deref());
    let active_output_secure =
        template::active_output_requires_keystore(&send_hosts, &selected_output, &exporter);
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
        hosts_panel_html,
        secrets_panel_html,
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
        let secret_ids = load_secret_ids(&state).await;
        let viewer_hosts = viewer_host_names(&hosts);
        let role_options = host_role_options();
        let row_id = next_row_id(signal_state.map(|s| &s.hosts), hosts.len());
        let row = blank_host_row();

        return sse_response(vec![
            row_signal_patch("hosts", row_id, &host_draft(&row, None)).as_datastar_event().to_string(),
            patch_host_error_event(""),
            patch_append_event(
                "#hosts-table-body",
                render_host_row(
                    row_id,
                    &row,
                    &secret_ids,
                    &viewer_hosts,
                    &role_options,
                    false,
                    true,
                    false,
                ),
            )
            .to_string(),
        ]);
    }

    let row_id = match id.parse::<usize>() {
        Ok(value) => value,
        Err(_) => {
            log::warn!("Rejected host action '{}' with non-numeric id '{}'", action, id);
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
            let secret_ids = load_secret_ids(&state).await;
            let viewer_hosts = viewer_host_names(&hosts);
            let role_options = host_role_options();
            sse_response(vec![
                row_signal_patch("hosts", row_id, &host_draft(&host, Some(&host.name)))
                    .as_datastar_event()
                    .to_string(),
                patch_host_error_event(""),
                patch_row_event(
                    &host_row_selector(row_id),
                    render_host_row(
                        row_id,
                        &host,
                        &secret_ids,
                        &viewer_hosts,
                        &role_options,
                        false,
                        true,
                        true,
                    ),
                )
                .to_string(),
            ])
        }
        "cancel" => {
            let Some(host) = host_row_by_id(&hosts, row_id).cloned() else {
                log::warn!("Host cancel for transient row {} removed patched row", row_id);
                return clear_and_remove_row("hosts", &host_row_selector(row_id), row_id);
            };
            let secret_ids = load_secret_ids(&state).await;
            let viewer_hosts = viewer_host_names(&hosts);
            let role_options = host_role_options();
            sse_response(vec![
                clear_row_signal_patch("hosts", row_id).as_datastar_event().to_string(),
                patch_host_error_event(""),
                patch_row_event(
                    &host_row_selector(row_id),
                    render_host_row(
                        row_id,
                        &host,
                        &secret_ids,
                        &viewer_hosts,
                        &role_options,
                        false,
                        false,
                        true,
                    ),
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
            match apply_upsert_host(&state, draft).await {
                Ok(_) => patch_host_row_saved_response(&state, row_id, &host_name).await,
                Err(err) => {
                    log::warn!("Host update for row {} failed: {}", row_id, err);
                    sse_response(vec![patch_host_error_event(&err)])
                }
            }
        }
        "delete" => delete_host_row(&hosts, row_id).await,
        _ => {
            log::warn!("Rejected unsupported host action '{}' for row {}", action, row_id);
            (
                StatusCode::BAD_REQUEST,
                "unsupported action; supported: create, read, cancel, update, delete",
            )
                .into_response()
        }
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

        let secrets = read_secret_rows().map(|(rows, _)| rows).unwrap_or_default();
        let row_id = next_row_id(signal_state.map(|s| &s.secrets), secrets.len());
        let row = blank_secret_row();

        return sse_response(vec![
            row_signal_patch("secrets", row_id, &secret_draft(&row, None)).as_datastar_event().to_string(),
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
    let (secrets, _) = match read_secret_rows() {
        Ok(result) => result,
        Err(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
    };

    match action.as_str() {
        "read" => {
            let Some(secret) = secret_row_by_id(&secrets, row_id).cloned() else {
                return (StatusCode::NOT_FOUND, "secret row not found").into_response();
            };
            sse_response(vec![
                row_signal_patch("secrets", row_id, &secret_draft(&secret, Some(&secret.secret_id)))
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
                clear_row_signal_patch("secrets", row_id).as_datastar_event().to_string(),
                patch_row_event(
                    &secret_row_selector(row_id),
                    render_secret_row(row_id, &secret, false, true, false),
                )
                .to_string(),
            ])
        }
        "update" => {
            let Some(draft) = secret_draft_from_signals(signal_state, row_id) else {
                return (StatusCode::BAD_REQUEST, "missing secret row draft signals").into_response();
            };
            match apply_upsert_secret(&state, draft).await {
                Ok(_) => {
                    patch_hosts_and_secrets_with_clear_response(&state, Some(("secrets", row_id))).await
                }
                Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
            }
        }
        "delete" => delete_secret_row(&state, &secrets, row_id).await,
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
    match apply_upsert_host(&state, form).await {
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
    match apply_upsert_host(&state, form).await {
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
    match apply_upsert_host(&state, form).await {
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
    Form(form): Form<SecretUpsertForm>,
) -> Response {
    match apply_upsert_secret(&state, form).await {
        Ok(_) => patch_hosts_and_secrets_response(&state).await,
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_secret(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<SecretDeleteForm>,
) -> Response {
    if !state.is_keystore_unlocked().await {
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before deleting secrets.",
        )
            .into_response();
    }

    let password = match get_password_from_env() {
        Ok(password) => password,
        Err(err) => return (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    };

    match remove_secret(form.secret_id.trim(), None, &password) {
        Ok(_) => {
            state.touch_keystore_session().await;
            patch_hosts_and_secrets_response(&state).await
        }
        Err(err) => (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    }
}

fn read_host_rows() -> Vec<template::HostsTableRow> {
    let mut rows = Vec::new();
    let hosts = KnownHost::parse_hosts_yml().unwrap_or_default();
    for (name, host) in hosts {
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
        rows.push(row);
    }
    rows
}

fn read_secret_rows() -> Result<(Vec<template::SecretTableRow>, Vec<String>), String> {
    let password = get_password_from_env().map_err(to_message)?;
    let secret_ids = list_secret_names(&password).map_err(to_message)?;
    let mut rows = Vec::new();

    for secret_id in &secret_ids {
        let auth = resolve_secret_auth(secret_id, &password).map_err(to_message)?;
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
    let viewer_hosts = viewer_host_names(hosts);
    let role_options = host_role_options();
    template::HostsTablePanelTemplate {
        keystore_locked,
        rows_html: hosts
            .iter()
            .enumerate()
            .map(|(idx, host)| {
                render_host_row(
                    idx + 1,
                    host,
                    secret_ids,
                    &viewer_hosts,
                    &role_options,
                    keystore_locked,
                    false,
                    true,
                )
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
    viewer_hosts: &[String],
    role_options: &[String],
    keystore_locked: bool,
    editing: bool,
    persisted: bool,
) -> String {
    template::HostsTableRowTemplate {
        row_id,
        host: host.clone(),
        secret_ids: secret_ids.to_vec(),
        viewer_hosts: viewer_hosts.to_vec(),
        role_options: role_options.to_vec(),
        keystore_locked,
        editing,
        persisted,
    }
    .render()
    .expect("render host row")
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
    Vec<template::SecretTableRow>,
    Vec<String>,
    bool,
) {
    let hosts = read_host_rows();
    let keystore_locked = state.keystore_status().await.0;
    let (secrets, secret_ids) = if keystore_locked {
        (Vec::new(), Vec::new())
    } else {
        match read_secret_rows() {
            Ok((rows, ids)) => {
                state.touch_keystore_session().await;
                (rows, ids)
            }
            Err(err) => {
                log::warn!("Failed to list keystore secrets for /hosts patch: {}", err);
                (Vec::new(), Vec::new())
            }
        }
    };

    (hosts, secrets, secret_ids, keystore_locked)
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

fn sse_response(events: Vec<String>) -> Response {
    ([(CONTENT_TYPE, "text/event-stream")], events.join("\n\n")).into_response()
}

async fn patch_hosts_panel_response(state: &Arc<ServerState>) -> Response {
    let (hosts, _secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => sse_response(vec![patch_panel_event("#hosts-table-panel", html)]),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

async fn patch_hosts_and_secrets_response(state: &Arc<ServerState>) -> Response {
    let (hosts, secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    let hosts_html = match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let secrets_html = match render_secrets_panel(&secrets, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };

    sse_response(vec![
        patch_panel_event("#hosts-table-panel", hosts_html),
        patch_panel_event("#secrets-table-panel", secrets_html),
    ])
}

async fn patch_hosts_and_secrets_with_clear_response(
    state: &Arc<ServerState>,
    clear: Option<(&str, usize)>,
) -> Response {
    let (hosts, secrets, secret_ids, keystore_locked) = load_table_panel_data(state).await;
    let hosts_html = match render_hosts_panel(&hosts, &secret_ids, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let secrets_html = match render_secrets_panel(&secrets, keystore_locked) {
        Ok(html) => html,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    };
    let mut events = Vec::new();
    if let Some((panel, row_id)) = clear {
        events.push(clear_row_signal_patch(panel, row_id).as_datastar_event().to_string());
    }
    events.push(patch_panel_event("#hosts-table-panel", hosts_html));
    events.push(patch_panel_event("#secrets-table-panel", secrets_html));
    sse_response(events)
}

async fn apply_upsert_host(state: &Arc<ServerState>, form: HostUpsertForm) -> Result<(), String> {
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
    let auth = form.auth.trim().to_ascii_lowercase();

    if let Some(secret_id) = &secret {
        if !state.is_keystore_unlocked().await {
            return Err("Unlock keystore before selecting a secret.".to_string());
        }
        let all_secret_ids = read_secret_rows()?.1;
        if !all_secret_ids.contains(secret_id) {
            return Err(format!("Unknown secret_id '{secret_id}'"));
        }
        state.touch_keystore_session().await;
    }

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
    if let Some(original_name) = form.original_name {
        let original_name = original_name.trim();
        if !original_name.is_empty() && original_name != name {
            hosts.remove(original_name);
        }
    }
    hosts.insert(name.to_string(), host);
    KnownHost::write_hosts_yml(&hosts).map_err(to_message)?;

    let mut settings = Settings::load().map_err(to_message)?;
    if settings
        .active_target
        .as_ref()
        .is_some_and(|target| !hosts.contains_key(target))
    {
        settings.active_target = hosts.keys().next().cloned();
        let _ = settings.save();
    }
    Ok(())
}

async fn apply_upsert_secret(
    state: &Arc<ServerState>,
    form: SecretUpsertForm,
) -> Result<(), String> {
    if !state.is_keystore_unlocked().await {
        return Err("Unlock keystore before editing secrets.".to_string());
    }

    let secret_id = form.secret_id.trim();
    if secret_id.is_empty() {
        return Err("secret_id is required.".to_string());
    }
    let password = get_password_from_env().map_err(to_message)?;

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
    state.touch_keystore_session().await;
    Ok(())
}

fn host_row_selector(row_id: usize) -> String {
    format!("#hosts-table-row-{row_id}")
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

fn blank_secret_row() -> template::SecretTableRow {
    template::SecretTableRow {
        secret_id: String::new(),
        auth_type: "ApiKey".to_string(),
        username: String::new(),
    }
}

fn viewer_host_names(hosts: &[template::HostsTableRow]) -> Vec<String> {
    hosts.iter()
        .filter(|host| host.has_role(&"view".to_string()))
        .map(|host| host.name.clone())
        .collect()
}

fn host_role_options() -> Vec<String> {
    vec!["collect".to_string(), "send".to_string(), "view".to_string()]
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
        apikey: Some(host.apikey.clone()),
        username: Some(host.username.clone()),
        password: Some(host.password.clone()),
        accept_invalid_certs: host.accept_invalid_certs,
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
    let row = signals?
        .rows
        .get(&row_id.to_string())?
        .draft
        .clone()?;
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
    match read_secret_rows() {
        Ok((_, ids)) => {
            state.touch_keystore_session().await;
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
        log::warn!("Host delete for transient row {} removed patched row", row_id);
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
                log::warn!("Host delete for row {} failed: {}", row_id, message);
                sse_response(vec![patch_host_error_event(&message)])
            }
        };
    }

    let message = host_map.err().unwrap_or_default();
    log::warn!("Host delete for row {} failed: {}", row_id, message);
    sse_response(vec![patch_host_error_event(&message)])
}

async fn delete_secret_row(
    state: &Arc<ServerState>,
    secrets: &[template::SecretTableRow],
    row_id: usize,
) -> Response {
    let Some(secret) = secret_row_by_id(secrets, row_id) else {
        return clear_and_remove_row("secrets", &secret_row_selector(row_id), row_id);
    };
    if !state.is_keystore_unlocked().await {
        return (
            StatusCode::PRECONDITION_FAILED,
            "Unlock keystore before deleting secrets.",
        )
            .into_response();
    }

    let password = match get_password_from_env() {
        Ok(password) => password,
        Err(err) => return (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    };

    match remove_secret(secret.secret_id.trim(), None, &password) {
        Ok(_) => {
            state.touch_keystore_session().await;
            patch_hosts_and_secrets_with_clear_response(state, Some(("secrets", row_id))).await
        }
        Err(err) => (StatusCode::BAD_REQUEST, to_message(err)).into_response(),
    }
}

fn clear_and_remove_row(panel: &str, selector: &str, row_id: usize) -> Response {
    sse_response(vec![
        clear_row_signal_patch(panel, row_id).as_datastar_event().to_string(),
        PatchElements::new_remove(selector).as_datastar_event().to_string(),
    ])
}

async fn patch_host_row_saved_response(
    state: &Arc<ServerState>,
    row_id: usize,
    host_name: &str,
) -> Response {
    let hosts = read_host_rows();
    let Some(host) = hosts.iter().find(|host| host.name == host_name).cloned() else {
        let message = format!("Saved host '{}' but could not reload the updated row.", host_name);
        log::warn!("{}", message);
        return sse_response(vec![patch_host_error_event(&message)]);
    };
    let keystore_locked = state.keystore_status().await.0;
    let viewer_hosts = viewer_host_names(&hosts);
    let role_options = host_role_options();

    sse_response(vec![
        clear_row_signal_patch("hosts", row_id).as_datastar_event().to_string(),
        patch_host_error_event(""),
        patch_row_event(
            &host_row_selector(row_id),
            render_host_row(
                row_id,
                &host,
                &[],
                &viewer_hosts,
                &role_options,
                keystore_locked,
                false,
                true,
            ),
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

fn to_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn to_message(err: impl std::fmt::Display) -> String {
    err.to_string()
}
