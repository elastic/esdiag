use super::{
    ServerState, Signals, Tab, api_key, append_body_event, execute_script_event, file_upload,
    html_event, service_link, signal_event,
};
use crate::data::{KnownHost, Settings, authenticate, keystore_exists};
use crate::server::template::{
    KeystoreBootstrapModal, KeystoreProcessUnlockModal, KeystoreUnlockModal,
};
use askama::Template;
use axum::{
    extract::{Form, State},
    http::{HeaderMap, HeaderValue, header::SET_COOKIE},
    response::{IntoResponse, Response},
};
use datastar::axum::ReadSignals;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub(crate) struct KeystoreForm {
    #[serde(default)]
    password: String,
    #[serde(default)]
    confirm: Option<String>,
}

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

pub async fn get_process_unlock_modal(State(state): State<Arc<ServerState>>) -> Response {
    let modal = KeystoreProcessUnlockModal {};
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
    Form(form): Form<KeystoreForm>,
) -> Response {
    if keystore_exists().unwrap_or(false) {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    if form.confirm.is_none() {
        state.publish_event(signal_event(
            r#"{"message":"Please confirm keystore creation to continue."}"#,
        ));
        state.publish_event(signal_event(r#"{"keystore":{"invalid":true}}"#));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    let password = form.password.trim().to_string();
    if password.len() < 6 {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password must be at least 6 characters."}"#,
        ));
        state.publish_event(signal_event(
            r#"{"keystore":{"password":"","invalid":true}}"#,
        ));
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
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/hosts') { window.location.reload(); }",
    ));
    state.publish_event(html_event(
        r#"<div id="keystore-bootstrap-modal" data-init="document.getElementById('keystore-bootstrap-modal')?.remove();"></div>"#,
    ));
    (
        axum::http::StatusCode::NO_CONTENT,
        keystore_session_header(),
    )
        .into_response()
}

pub async fn unlock(
    State(state): State<Arc<ServerState>>,
    Form(form): Form<KeystoreForm>,
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

    let password = form.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password is required."}"#,
        ));
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked(password).await;
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":false}}"#,
            ));
            state.publish_event(execute_script_event(
                "if (window.location.pathname === '/hosts') { window.location.reload(); }",
            ));
            state.publish_event(html_event(
                r#"<div id="keystore-unlock-modal" data-init="document.getElementById('keystore-unlock-modal')?.remove();"></div>"#,
            ));
            (
                axum::http::StatusCode::NO_CONTENT,
                keystore_session_header(),
            )
                .into_response()
        }
        Err(err) => {
            state.record_keystore_failed_attempt().await;
            state.publish_event(signal_event(
                r#"{"message":"Invalid keystore password. Try again."}"#,
            ));
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":true}}"#,
            ));
            log::warn!("Keystore unlock failed: {}", err);
            axum::http::StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

pub async fn unlock_and_run(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<Signals>,
) -> Response {
    let password = signals.keystore.password.trim().to_string();
    if password.is_empty() {
        state.publish_event(signal_event(
            r#"{"message":"Keystore password is required."}"#,
        ));
        state.publish_event(signal_event(r#"{"keystore":{"invalid":true}}"#));
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }

    match authenticate(&password) {
        Ok(_) => {
            state.set_keystore_unlocked(password).await;
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":false}}"#,
            ));
            state.publish_event(html_event(
                r#"<div id="keystore-process-unlock-modal" data-init="document.getElementById('keystore-process-unlock-modal')?.remove();"></div>"#,
            ));

            match signals.tab {
                Tab::FileUpload => {
                    file_upload::process(State(state), headers, ReadSignals(signals))
                        .await
                        .into_response()
                }
                Tab::ServiceLink => {
                    service_link::form(State(state), headers, ReadSignals(signals))
                        .await
                        .into_response()
                }
                Tab::ApiKey => {
                    api_key::form(State(state), headers, ReadSignals(signals))
                        .await
                        .into_response()
                }
            }
        }
        Err(err) => {
            state.record_keystore_failed_attempt().await;
            state.publish_event(signal_event(
                r#"{"message":"Invalid keystore password. Try again."}"#,
            ));
            state.publish_event(signal_event(
                r#"{"keystore":{"password":"","invalid":true}}"#,
            ));
            log::warn!("Keystore unlock-and-run failed: {}", err);
            axum::http::StatusCode::NO_CONTENT.into_response()
        }
    }
}

pub async fn lock(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    state.set_keystore_locked("manual").await;
    state.publish_event(execute_script_event(
        "if (window.location.pathname === '/hosts') { window.location.reload(); }",
    ));
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

#[cfg(test)]
mod tests {
    use super::{KeystoreForm, lock, unlock};
    use crate::data::authenticate;
    use crate::server::{ServerEvent, test_server_state};
    use axum::{
        extract::{Form, State},
        http::{HeaderValue, StatusCode, header::SET_COOKIE},
        response::IntoResponse,
    };
    use std::{
        path::PathBuf,
        sync::Mutex,
    };
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let keystore_path = config_dir.join("secrets.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
        }
        (tmp, keystore_path)
    }

    #[tokio::test]
    async fn unlock_is_idempotent_and_emits_cookie_and_signal_updates() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");
        assert!(keystore_path.is_file(), "keystore should exist");

        let state = test_server_state();
        let mut events = state.subscribe_events();

        let response = unlock(
            State(state.clone()),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(SET_COOKIE),
            Some(&HeaderValue::from_static(
                "keystore_session=1; Path=/; Max-Age=43200; SameSite=Lax"
            ))
        );
        assert!(
            !state.keystore_status().await.0,
            "unlock should transition state to unlocked"
        );

        let first = events.recv().await.expect("unlock signal");
        match first {
            ServerEvent::Signals(payload) => {
                assert!(payload.contains(r#""keystore":{"locked":false"#));
            }
            other => panic!("expected keystore signal, got {other:?}"),
        }

        let response = unlock(
            State(state.clone()),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            !state.keystore_status().await.0,
            "repeat unlock should remain unlocked"
        );
    }

    #[tokio::test]
    async fn unlock_rejects_invalid_password_with_401_and_stays_locked() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let state = test_server_state();
        let response = unlock(
            State(state.clone()),
            Form(KeystoreForm {
                password: "wrong".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(state.keystore_status().await.0, "state should remain locked");
    }

    #[tokio::test]
    async fn unlock_requires_existing_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, keystore_path) = setup_env();
        assert!(!keystore_path.exists(), "keystore should start missing");

        let state = test_server_state();
        let response = unlock(
            State(state),
            Form(KeystoreForm {
                password: "pw".to_string(),
                confirm: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    }

    #[tokio::test]
    async fn lock_is_idempotent_and_preserves_locked_state() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path) = setup_env();
        let state = test_server_state();
        state.set_keystore_unlocked("pw".to_string()).await;

        let response = lock(State(state.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "lock should relock state");

        let response = lock(State(state.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(state.keystore_status().await.0, "repeat lock stays locked");
    }
}
