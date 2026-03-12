#[cfg(feature = "keystore")]
use super::keystore;
use super::{ServerState, append_body_event, execute_script_event, html_event};
use crate::data::{KnownHost, Settings, Uri, with_scoped_keystore_password};
use crate::exporter::Exporter;
use crate::server::template::SettingsModal;
use askama::Template;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_modal(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let can_manage_hosts = state.runtime_mode_policy.allows_host_management();
    let can_update_exporter = state.runtime_mode_policy.allows_exporter_updates();

    let settings = if state.runtime_mode_policy.allows_local_artifacts() {
        Settings::load().unwrap_or_default()
    } else {
        Settings::default()
    };

    // In service mode, avoid hosts.yml reads entirely.
    let hosts = if can_manage_hosts {
        KnownHost::list_all().unwrap_or_default()
    } else {
        vec![state.exporter.read().await.to_string()]
    };

    let active_target = if can_update_exporter {
        settings.active_target.clone().unwrap_or_default()
    } else {
        state.exporter.read().await.to_string()
    };
    let kibana_url = state.kibana_url.read().await.clone();

    let modal = SettingsModal {
        hosts,
        active_target,
        kibana_url,
        mode: state.runtime_mode.to_string(),
        can_update_exporter,
    };

    match modal.render() {
        Ok(html) => state.publish_event(append_body_event(html)),
        Err(err) => state.publish_event(html_event(format!("<div>Error: {}</div>", err))),
    }
    StatusCode::NO_CONTENT
}

#[derive(Deserialize, Default)]
pub struct UpdateSettingsForm {
    target: String,
    kibana_url: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    datastar::axum::ReadSignals(signals): datastar::axum::ReadSignals<super::Signals>,
) -> Response {
    if !state.runtime_mode_policy.allows_local_artifacts() {
        let form = signals.settings;
        if let Some(kibana) = form.kibana_url {
            *state.kibana_url.write().await = kibana;
        }
        clear_settings_errors(&state);
        state.publish_event(html_event(
            r#"
            <div id="settings-modal" data-init="window.location.reload();">
                Reloading...
            </div>
            "#,
        ));
        return StatusCode::NO_CONTENT.into_response();
    }

    let mut settings = Settings::load().unwrap_or_default();
    let form = signals.settings;

    // 1. Process target selection
    if form.target == "new_host" {
        let err_msg = "Inline host creation from output settings is no longer supported. Use /settings instead.".to_string();
        log::warn!("{}", err_msg);
        return settings_error_response(&state, err_msg);
    } else {
        settings.active_target = KnownHost::get_known(&form.target).map(|_| form.target.clone());
    }

    // 2. Process kibana URL
    if let Some(kibana) = form.kibana_url {
        settings.kibana_url = Some(kibana.clone());
        *state.kibana_url.write().await = kibana;
    }

    // 3. Save settings to disk
    if let Err(e) = settings.save() {
        let err_msg = format!("Failed to save settings: {}", e);
        log::error!("{}", err_msg);
        return settings_error_response(&state, err_msg);
    }

    // 4. Update the active Exporter in ServerState (user mode only)
    if state.runtime_mode_policy.allows_exporter_updates() {
        let target = form.target.clone();
        let current_exporter = state.exporter.read().await.clone();
        let request_user = state
            .resolve_user_email(&headers)
            .map(|(_, user)| user)
            .unwrap_or_else(|_| "Anonymous".to_string());
        let keystore_password = state.keystore_password_for(&request_user).await;

        let next_exporter = if let Some(host) = KnownHost::get_known(&target) {
            if host_requires_keystore(&host) && keystore_password.is_none() {
                return secure_host_unlock_required_response(
                    &state,
                    headers.clone(),
                    "Keystore is locked. Unlock it before selecting secure saved outputs."
                        .to_string(),
                )
                .await;
            }

            if let Some(password) = keystore_password {
                with_scoped_keystore_password(password, async move {
                    Exporter::try_from(host)
                        .map_err(|e| format!("Failed to construct exporter: {}", e))
                })
                .await
            } else {
                Exporter::try_from(host).map_err(|e| format!("Failed to construct exporter: {}", e))
            }
        } else if target == current_exporter.target_value() {
            Ok(current_exporter)
        } else {
            let exporter_uri = match Uri::try_from(target.clone()) {
                Ok(uri) => uri,
                Err(e) => {
                    let err_msg = format!("Invalid output target: {}", e);
                    log::error!("{}", err_msg);
                    return settings_error_response(&state, err_msg);
                }
            };
            Exporter::try_from(exporter_uri)
                .map_err(|e| format!("Failed to construct exporter: {}", e))
        };

        match next_exporter {
            Ok(new_exporter) => {
                *state.exporter.write().await = new_exporter;
            }
            Err(err_msg) => {
                log::error!("{}", err_msg);
                return settings_error_response(&state, err_msg);
            }
        }
    }

    // 5. Build response to remove modal and update exporter text
    clear_settings_errors(&state);
    let html = r#"
        <div id="settings-modal" data-init="window.location.reload();">
            Reloading...
        </div>
        "#;
    state.publish_event(html_event(html));

    StatusCode::NO_CONTENT.into_response()
}

fn settings_error_response(state: &Arc<ServerState>, err_msg: String) -> Response {
    state.publish_event(execute_script_event(render_settings_error_script(&err_msg)));
    StatusCode::NO_CONTENT.into_response()
}

async fn secure_host_unlock_required_response(
    state: &Arc<ServerState>,
    headers: HeaderMap,
    err_msg: String,
) -> Response {
    #[cfg(feature = "keystore")]
    let _ = keystore::get_unlock_modal(State(state.clone()), headers).await;
    #[cfg(not(feature = "keystore"))]
    let _ = headers;
    settings_error_response(state, err_msg)
}

fn clear_settings_errors(state: &Arc<ServerState>) {
    state.publish_event(execute_script_event(render_settings_error_script("")));
}

fn host_requires_keystore(host: &KnownHost) -> bool {
    !matches!(host, KnownHost::NoAuth { .. })
}

fn render_settings_error_script(err_msg: &str) -> String {
    let message = serde_json::to_string(err_msg).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"
            (() => {{
                const message = {message};
                const targetIds = ["settings-form-error", "footer-settings-error"];
                targetIds.forEach((id) => {{
                    const target = document.getElementById(id);
                    if (!target) return;
                    target.replaceChildren();
                    if (!message) return;
                    const wrapper = document.createElement("div");
                    wrapper.className = "error";
                    wrapper.setAttribute("role", "alert");
                    const text = document.createElement("p");
                    text.textContent = message;
                    wrapper.appendChild(text);
                    target.appendChild(wrapper);
                }});
            }})();
        "#
    )
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::update_settings;
    use crate::{
        data::{KnownHost, Product, Settings, authenticate},
        server::{ServerEvent, Signals, test_server_state},
    };
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
    };
    use datastar::axum::ReadSignals;
    use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, PathBuf, PathBuf) {
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
        (tmp, hosts_path, keystore_path)
    }

    fn write_hosts(hosts: BTreeMap<String, KnownHost>) {
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
    }

    #[tokio::test]
    async fn secure_saved_host_selection_prompts_unlock_when_locked() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();
        authenticate("pw").expect("create keystore");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "secure-es".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: None,
                app: Product::Elasticsearch,
                cloud_id: None,
                roles: vec![],
                secret: Some("secure-es".to_string()),
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        Settings {
            active_target: Some("stdout".to_string()),
            kibana_url: None,
        }
        .save()
        .expect("save settings");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let mut signals = Signals::default();
        signals.settings.target = "secure-es".to_string();

        let response = update_settings(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let mut saw_unlock_modal = false;
        let mut saw_unlock_message = false;
        while let Ok(event) = events.try_recv() {
            match event {
                ServerEvent::AppendBody(html) => {
                    if html.contains("keystore-unlock-modal") {
                        saw_unlock_modal = true;
                    }
                }
                ServerEvent::ExecuteScript(script) => {
                    if script.contains("Unlock it before selecting secure saved outputs") {
                        saw_unlock_message = true;
                    }
                }
                _ => {}
            }
        }

        assert!(saw_unlock_modal, "expected unlock modal to be shown");
        assert!(saw_unlock_message, "expected unlock-required error message");
    }
}
