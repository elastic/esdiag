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

fn clear_settings_errors(state: &Arc<ServerState>) {
    state.publish_event(execute_script_event(render_settings_error_script("")));
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
