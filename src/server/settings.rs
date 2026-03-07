use super::{
    ServerState, append_body_event, html_event, prepend_selector_event,
};
use crate::data::{KnownHost, KnownHostBuilder, Settings, Uri};
use crate::exporter::Exporter;
use crate::server::template::SettingsModal;
use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
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
    new_host_name: Option<String>,
    new_host_url: Option<String>,
    new_host_apikey: Option<String>,
    new_host_username: Option<String>,
    new_host_password: Option<String>,
    kibana_url: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<ServerState>>,
    datastar::axum::ReadSignals(signals): datastar::axum::ReadSignals<super::Signals>,
) -> Response {
    if !state.runtime_mode_policy.allows_local_artifacts() {
        let form = signals.settings;
        if let Some(kibana) = form.kibana_url {
            *state.kibana_url.write().await = kibana;
        }
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
        // Build new host
        if let (Some(name), Some(url)) = (&form.new_host_name, &form.new_host_url)
            && !name.is_empty()
            && !url.is_empty()
        {
            match url.parse() {
                Ok(parsed_url) => {
                    let mut builder = KnownHostBuilder::new(parsed_url);

                    if let Some(apikey) = &form.new_host_apikey {
                        if !apikey.is_empty() {
                            builder = builder.apikey(Some(apikey.clone()));
                        }
                    } else if let (Some(user), Some(pass)) =
                        (&form.new_host_username, &form.new_host_password)
                        && !user.is_empty()
                        && !pass.is_empty()
                    {
                        builder = builder
                            .username(Some(user.clone()))
                            .password(Some(pass.clone()));
                    }

                    match builder.build() {
                        Ok(host) => {
                            // Validate connection before saving
                            let is_valid = match Uri::try_from(host.clone()) {
                                Ok(uri) => match crate::client::Client::try_from(uri) {
                                    Ok(client) => match client.test_connection().await {
                                        Ok(_) => true,
                                        Err(e) => {
                                            let err_msg =
                                                format!("Failed to connect to new host: {}", e);
                                            log::error!("{}", err_msg);
                                            return settings_error_response(&state, err_msg);
                                        }
                                    },
                                    Err(e) => {
                                        let err_msg = format!("Failed to construct client: {}", e);
                                        log::error!("{}", err_msg);
                                        return settings_error_response(&state, err_msg);
                                    }
                                },
                                Err(e) => {
                                    let err_msg = format!("Failed to parse host into URI: {}", e);
                                    log::error!("{}", err_msg);
                                    return settings_error_response(&state, err_msg);
                                }
                            };

                            if is_valid {
                                match host.save(name) {
                                    Ok(_) => {
                                        settings.active_target = Some(name.clone());
                                    }
                                    Err(e) => {
                                        let err_msg =
                                            format!("Failed to save host to hosts.yml: {}", e);
                                        log::error!("{}", err_msg);
                                        return settings_error_response(&state, err_msg);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("Failed to configure host: {}", e);
                            log::error!("{}", err_msg);
                            return settings_error_response(&state, err_msg);
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!("Invalid URL provided for new host: {}", e);
                    log::error!("{}", err_msg);
                    return settings_error_response(&state, err_msg);
                }
            }
        }
    } else {
        settings.active_target = Some(form.target.clone());
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
    if state.runtime_mode_policy.allows_exporter_updates() && let Some(target) = &settings.active_target {
        match KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found")) {
            Ok(host) => match Uri::try_from(host) {
                Ok(uri) => match Exporter::try_from(uri) {
                    Ok(new_exporter) => {
                        *state.exporter.write().await = new_exporter;
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to construct exporter: {}", e);
                        log::error!("{}", err_msg);
                        return settings_error_response(&state, err_msg);
                    }
                },
                Err(e) => {
                    let err_msg = format!("Invalid Host URI: {}", e);
                    log::error!("{}", err_msg);
                    return settings_error_response(&state, err_msg);
                }
            },
            Err(e) => {
                let err_msg = format!("Could not find Target in hosts.yml: {}", e);
                log::error!("{}", err_msg);
                return settings_error_response(&state, err_msg);
            }
        }
    }

    // 5. Build response to remove modal and update exporter text
    let html = r#"
        <div id="settings-modal" data-init="window.location.reload();">
            Reloading...
        </div>
        "#;
    state.publish_event(html_event(html));

    StatusCode::NO_CONTENT.into_response()
}

fn settings_error_response(state: &Arc<ServerState>, err_msg: String) -> Response {
    state.publish_event(prepend_selector_event(
        "#settings-form",
        format!(
            "<div id='settings-error' style='color: red; padding: 10px;'>{}</div>",
            err_msg
        ),
    ));
    StatusCode::NO_CONTENT.into_response()
}
