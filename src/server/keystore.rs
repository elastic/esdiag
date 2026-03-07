use super::{
    ServerState, append_body_event, html_event, signal_event,
};
use crate::data::{KnownHost, Settings, authenticate, keystore_exists};
use crate::server::template::{KeystoreBootstrapModal, KeystoreUnlockModal};
use askama::Template;
use axum::{
    extract::State,
    http::{
        HeaderMap, HeaderValue,
        header::SET_COOKIE,
    },
    response::{IntoResponse, Response},
};
use datastar::axum::ReadSignals;
use std::sync::Arc;

pub async fn get_unlock_modal(State(state): State<Arc<ServerState>>) -> Response {
    if !keystore_exists().unwrap_or(false) {
        return get_bootstrap_modal(State(state)).await;
    }
    let modal = KeystoreUnlockModal {};
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

fn hosts_file_exists() -> bool {
    let hosts_path = KnownHost::get_hosts_path();
    hosts_path.is_file()
}

fn migration_needed() -> bool {
    hosts_file_exists() && !keystore_exists().unwrap_or(false)
}

fn keystore_session_header() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let cookie = "keystore_session=1; Path=/; Max-Age=43200; SameSite=Lax";
    if let Ok(value) = HeaderValue::from_str(cookie) {
        headers.append(SET_COOKIE, value);
    }
    headers
}

pub async fn get_bootstrap_modal(State(state): State<Arc<ServerState>>) -> Response {
    if keystore_exists().unwrap_or(false) {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    let modal = KeystoreBootstrapModal {
        migrate: migration_needed(),
    };
    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub async fn bootstrap(
    State(state): State<Arc<ServerState>>,
    ReadSignals(signals): ReadSignals<super::Signals>,
) -> Response {
    if keystore_exists().unwrap_or(false) {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    if !signals.keystore.confirm {
        state.publish_event(signal_event(
            r#"{"message":"Please confirm keystore creation to continue."}"#,
        ));
        state.publish_event(signal_event(r#"{"keystore":{"invalid":true}}"#));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    let password = signals.keystore.password.trim().to_string();
    if password.len() < 6 {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password must be at least 6 characters."}"#,
        ));
        state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":true}}"#));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    if let Err(err) = authenticate(&password) {
        state.publish_event(signal_event(format!(
            r#"{{"message":"Failed to initialize keystore: {}"}}"#,
            err
        )));
        log::error!("Keystore bootstrap failed: {}", err);
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if migration_needed() {
        if let Err(err) = KnownHost::migrate_hosts_to_keystore(&password) {
            state.publish_event(signal_event(format!(
                r#"{{"message":"Failed to migrate hosts to keystore: {}"}}"#,
                err
            )));
            log::error!("Keystore migration failed: {}", err);
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    state.set_keystore_unlocked(password).await;
    state.publish_event(signal_event(
        r#"{"keystore":{"password":"","invalid":false,"confirm":false}}"#,
    ));
    state.publish_event(html_event(
        r#"<div id="keystore-bootstrap-modal" data-init="document.getElementById('keystore-bootstrap-modal')?.remove();"></div>"#,
    ));
    (axum::http::StatusCode::NO_CONTENT, keystore_session_header()).into_response()
}

pub async fn unlock(
    State(state): State<Arc<ServerState>>,
    ReadSignals(signals): ReadSignals<super::Signals>,
) -> Response {
    if !keystore_exists().unwrap_or(false) {
        state.publish_event(signal_event(
            r#"{"message":"Create a keystore before unlocking."}"#,
        ));
        let _ = get_bootstrap_modal(State(state.clone())).await;
        return axum::http::StatusCode::PRECONDITION_FAILED.into_response();
    }

    if let Some(blocked_until) = state.keystore_blocked_until().await {
        let now = chrono::Utc::now().timestamp();
        if blocked_until > now {
            let retry_after = blocked_until - now;
            state.publish_event(signal_event(format!(
                r#"{{"message":"Keystore temporarily locked. Retry in {} seconds."}}"#,
                retry_after
            )));
            return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }

    let password = signals.keystore.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password is required."}"#,
        ));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked(password).await;
            state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":false}}"#));
            state.publish_event(html_event(
                r#"<div id="keystore-unlock-modal" data-init="document.getElementById('keystore-unlock-modal')?.remove();"></div>"#,
            ));
            (axum::http::StatusCode::NO_CONTENT, keystore_session_header()).into_response()
        }
        Err(err) => {
            state.record_keystore_failed_attempt().await;
            state.publish_event(signal_event(
                r#"{"message":"Invalid keystore password. Try again."}"#,
            ));
            state.publish_event(signal_event(r#"{"keystore":{"password":"","invalid":true}}"#));
            log::warn!("Keystore unlock failed: {}", err);
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

pub async fn lock(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    state.set_keystore_locked("manual").await;
    axum::http::StatusCode::NO_CONTENT
}

pub async fn ensure_unlocked_for_active_output(state: &Arc<ServerState>) -> Result<(), String> {
    if !state.runtime_mode_policy.allows_local_artifacts() {
        return Err("Keystore unavailable in service mode.".to_string());
    }

    let settings = Settings::load().unwrap_or_default();
    let Some(target) = settings.active_target else {
        return Ok(());
    };
    let Some(host) = KnownHost::get_known(&target) else {
        return Ok(());
    };

    let secure = !matches!(host, KnownHost::NoAuth { .. });
    if !secure {
        return Ok(());
    }

    if !state.is_keystore_unlocked().await {
        return Err("Keystore is locked. Unlock it before processing secure outputs.".to_string());
    }

    state.touch_keystore_session().await;
    Ok(())
}
