use super::{ServerState, get_theme_dark, signal_event, template};
use crate::data::{
    ElasticCloud, HostRole, KnownHost, Product, SecretAuth, Settings, get_password_from_env,
    list_secret_names, remove_secret, resolve_secret_auth, upsert_secret_auth,
};
use askama::Template;
use axum::{
    extract::{Form, Json, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, str::FromStr, sync::Arc};
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
    let hosts_records_json = build_hosts_signal_json(&hosts);
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
    let secrets_records_json = build_secrets_signal_json(&secrets);

    let page = template::HostsPage {
        auth_header,
        debug: log::max_level() >= log::LevelFilter::Debug,
        desktop: cfg!(feature = "desktop"),
        can_configure_output: state.runtime_mode_policy.allows_exporter_updates(),
        send_hosts: KnownHost::list_by_role(HostRole::Send).unwrap_or_default(),
        exporter: state.exporter.read().await.to_string(),
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
        hosts,
        secrets,
        secret_ids,
        hosts_records_json,
        secrets_records_json,
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

pub async fn upsert_host(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<HostUpsertForm>,
) -> Response {
    match apply_upsert_host(&state, form).await {
        Ok(_) => Redirect::to("/hosts").into_response(),
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
        Ok(_) => {
            publish_hosts_records_patch(&state);
            StatusCode::NO_CONTENT.into_response()
        }
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
        Ok(_) => {
            publish_hosts_records_patch(&state);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, err).into_response(),
    }
}

pub async fn delete_host(Form(form): Form<HostDeleteForm>) -> Response {
    let mut hosts = KnownHost::parse_hosts_yml().map_err(to_message);
    if let Ok(ref mut hosts) = hosts {
        hosts.remove(form.name.trim());
        return match KnownHost::write_hosts_yml(hosts) {
            Ok(_) => Redirect::to("/hosts").into_response(),
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
        Ok(_) => Redirect::to("/hosts").into_response(),
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
            Redirect::to("/hosts").into_response()
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
        let auth_type = match auth {
            Some(SecretAuth::ApiKey { .. }) => "ApiKey",
            Some(SecretAuth::Basic { .. }) => "Basic",
            None => "Unknown",
        };
        rows.push(template::SecretTableRow {
            secret_id: secret_id.clone(),
            auth_type: auth_type.to_string(),
        });
    }

    Ok((rows, secret_ids))
}

fn publish_hosts_records_patch(state: &Arc<ServerState>) {
    let records = read_host_rows();
    let patch = json!({
        "hosts": {
            "records": records
        }
    });
    state.publish_event(signal_event(patch.to_string()));
}

fn build_hosts_signal_json(records: &[template::HostsTableRow]) -> String {
    json!({
        "columns": ["name","product","roles","viewer","auth","secret","insecure","url"],
        "records": records
    })
    .to_string()
}

fn build_secrets_signal_json(records: &[template::SecretTableRow]) -> String {
    json!({
        "columns": ["secret_id", "type"],
        "records": records
    })
    .to_string()
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
