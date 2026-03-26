#[cfg(feature = "keystore")]
use super::keystore;
use super::{ServerState, append_body_event, execute_script_event, html_event, signal_event};
use crate::data::{HostRole, KnownHost, Settings, Uri, with_scoped_keystore_password};
use crate::exporter::Exporter;
use crate::server::template::{self, SettingsModal};
use askama::Template;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

pub async fn get_modal(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let can_update_exporter = state.runtime_mode_policy.allows_exporter_updates();
    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let settings = if allows_local_runtime_features {
        Settings::load().unwrap_or_default()
    } else {
        Settings::default()
    };
    let exporter = state.exporter.read().await.clone();
    let (output_options, selected_output, _exporter_label) = if allows_local_runtime_features {
        let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
        let send_hosts: Vec<String> = hosts_by_name
            .iter()
            .filter(|(_, h)| h.has_role(HostRole::Send))
            .map(|(name, _)| name.clone())
            .collect();
        template::build_footer_output_context(
            &hosts_by_name,
            &send_hosts,
            &exporter,
            settings.active_target.as_deref(),
        )
    } else {
        let selected_output = exporter.target_uri();
        (
            vec![template::FooterOutputOption {
                value: selected_output.clone(),
                label: exporter.target_label(),
            }],
            selected_output,
            exporter.target_label(),
        )
    };
    let kibana_url = state.kibana_url.read().await.clone();

    let modal = SettingsModal {
        output_options,
        selected_output,
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
    target: Option<String>,
    kibana_url: Option<String>,
}

pub async fn update_settings(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    datastar::axum::ReadSignals(signals): datastar::axum::ReadSignals<super::Signals>,
) -> Response {
    let request_user = match state.resolve_user_email(&headers) {
        Ok((_, user)) => user,
        Err(err) if state.runtime_mode_policy.requires_iap_headers() => {
            tracing::warn!("Settings update denied: {}", err);
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Err(_) => "Anonymous".to_string(),
    };

    if !state.runtime_mode_policy.allows_local_runtime_features() {
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

    let settings = Settings::load().unwrap_or_default();
    let prior_active_target = settings.active_target.clone();
    let mut next_settings = settings.clone();
    let form = signals.settings;
    let target = form.target.as_deref().unwrap_or("").trim().to_string();
    let current_exporter = state.exporter.read().await.clone();
    let prior_effective_target = prior_active_target
        .clone()
        .unwrap_or_else(|| current_exporter.target_uri());
    let target_changed = !target.is_empty() && target != prior_effective_target;

    // 1. Process target selection
    if target == "new_host" {
        let err_msg = "Inline host creation from output settings is no longer supported. Use /settings instead.".to_string();
        tracing::warn!("{}", err_msg);
        return settings_error_response(&state, prior_active_target.as_deref(), err_msg).await;
    } else if target_changed {
        match KnownHost::get_known(&target) {
            Some(host) if host.has_role(HostRole::Send) => {
                next_settings.active_target = Some(target.clone());
            }
            Some(_) => {
                let err_msg = format!("Output target '{}' is not a send-capable host.", target);
                tracing::warn!("{}", err_msg);
                return settings_error_response(&state, prior_active_target.as_deref(), err_msg)
                    .await;
            }
            None => {
                next_settings.active_target = None;
            }
        }
    }

    // 2. Process kibana URL
    if let Some(kibana) = form.kibana_url {
        next_settings.kibana_url = Some(kibana.clone());
    }

    let mut validated_exporter = None;

    // 3. Validate and update the active Exporter in ServerState (user mode only)
    if state.runtime_mode_policy.allows_exporter_updates() {
        let keystore_password = state.keystore_password_for(&request_user).await;

        let next_exporter = if !target_changed {
            Ok(current_exporter)
        } else if let Some(host) = KnownHost::get_known(&target) {
            if host_requires_keystore(&host) && keystore_password.is_none() {
                return secure_host_unlock_required_response(
                    &state,
                    headers.clone(),
                    prior_active_target.as_deref(),
                    secure_saved_output_error_message(),
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
        } else if target == current_exporter.target_uri() {
            Ok(current_exporter)
        } else {
            let exporter_uri = match Uri::try_from(target.clone()) {
                Ok(uri) => uri,
                Err(e) => {
                    let err_msg = format!("Invalid output target: {}", e);
                    tracing::error!("{}", err_msg);
                    return settings_error_response(
                        &state,
                        prior_active_target.as_deref(),
                        err_msg,
                    )
                    .await;
                }
            };
            Exporter::try_from(exporter_uri)
                .map_err(|e| format!("Failed to construct exporter: {}", e))
        };

        match next_exporter {
            Ok(new_exporter) => {
                validated_exporter = Some(new_exporter);
            }
            Err(err_msg) => {
                tracing::error!("{}", err_msg);
                return settings_error_response(&state, prior_active_target.as_deref(), err_msg)
                    .await;
            }
        }
    }

    // 4. Save settings to disk after validation succeeds
    if let Err(e) = next_settings.save() {
        let err_msg = format!("Failed to save settings: {}", e);
        tracing::error!("{}", err_msg);
        return settings_error_response(&state, prior_active_target.as_deref(), err_msg).await;
    }

    if let Some(kibana_url) = next_settings.kibana_url.clone() {
        *state.kibana_url.write().await = kibana_url;
    }

    if let Some(new_exporter) = validated_exporter {
        *state.exporter.write().await = new_exporter;
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

async fn settings_error_response(
    state: &Arc<ServerState>,
    prior_active_target: Option<&str>,
    err_msg: String,
) -> Response {
    state.publish_event(signal_event(
        footer_selection_signal_payload(state, prior_active_target).await,
    ));
    state.publish_event(execute_script_event(render_settings_error_script(&err_msg)));
    StatusCode::NO_CONTENT.into_response()
}

async fn secure_host_unlock_required_response(
    state: &Arc<ServerState>,
    headers: HeaderMap,
    prior_active_target: Option<&str>,
    err_msg: String,
) -> Response {
    #[cfg(feature = "keystore")]
    let _ = keystore::get_unlock_modal(State(state.clone()), headers).await;
    #[cfg(not(feature = "keystore"))]
    let _ = headers;
    settings_error_response(state, prior_active_target, err_msg).await
}

fn clear_settings_errors(state: &Arc<ServerState>) {
    state.publish_event(execute_script_event(render_settings_error_script("")));
}

fn host_requires_keystore(host: &KnownHost) -> bool {
    host.requires_keystore_secret()
}

fn secure_saved_output_error_message() -> String {
    #[cfg(feature = "keystore")]
    {
        "Keystore is locked. Unlock it before selecting secure saved outputs.".to_string()
    }
    #[cfg(not(feature = "keystore"))]
    {
        "Keystore support is unavailable in this build, so secure saved outputs cannot be selected."
            .to_string()
    }
}

async fn footer_selection_signal_payload(
    state: &Arc<ServerState>,
    prior_active_target: Option<&str>,
) -> String {
    let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
    let send_hosts: Vec<String> = hosts_by_name
        .iter()
        .filter(|(_, h)| h.has_role(HostRole::Send))
        .map(|(name, _)| name.clone())
        .collect();
    let exporter = state.exporter.read().await.clone();
    let (_output_options, selected_output, _label) = template::build_footer_output_context(
        &hosts_by_name,
        &send_hosts,
        &exporter,
        prior_active_target,
    );
    let secure = template::active_output_requires_keystore(
        &hosts_by_name,
        &send_hosts,
        &selected_output,
        &exporter,
    );
    serde_json::json!({
        "settings": { "target": selected_output },
        "output": { "secure": secure }
    })
    .to_string()
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
    use super::{get_modal, update_settings};
    use crate::{
        data::{HostRole, KnownHost, Product, Settings, Uri, authenticate},
        exporter::Exporter,
        server::{RuntimeMode, RuntimeModePolicy, ServerEvent, Signals, test_server_state},
    };
    use axum::{
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use datastar::axum::ReadSignals;
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{Arc, Mutex},
    };
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
                roles: vec![HostRole::Send],
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
        let expected_target = state.exporter.read().await.target_uri();
        let mut events = state.subscribe_events();
        let mut signals = Signals::default();
        signals.settings.target = Some("secure-es".to_string());

        let response = update_settings(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let mut saw_unlock_modal = false;
        let mut saw_unlock_message = false;
        let mut saw_target_revert = false;
        let mut saw_secure_revert = false;
        while let Ok(event) = events.try_recv() {
            match event {
                ServerEvent::Signals(payload) => {
                    if payload
                        .contains(&format!(r#""settings":{{"target":"{}"}}"#, expected_target))
                    {
                        saw_target_revert = true;
                    }
                    if payload.contains(r#""output":{"secure":false}"#) {
                        saw_secure_revert = true;
                    }
                }
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
        assert!(saw_target_revert, "expected target selection to revert");
        assert!(saw_secure_revert, "expected secure indicator to revert");
    }

    #[tokio::test]
    async fn settings_modal_includes_live_exporter_option_when_no_saved_target_selected() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "saved-host".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);
        Settings::default().save().expect("save settings");

        let state = test_server_state();
        *state.exporter.write().await =
            Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/output")))
                .expect("directory exporter");
        let mut events = state.subscribe_events();

        let response = get_modal(State(state)).await.into_response();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let event = events.try_recv().expect("modal render event");
        let ServerEvent::AppendBody(html) = event else {
            panic!("expected modal html");
        };
        assert!(html.contains(r#"option value="file:///tmp/output/" selected"#));
        assert!(html.contains("dir: /tmp/output/"));
    }

    #[tokio::test]
    async fn collect_only_host_cannot_be_selected_as_output_target() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "collector-only".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect],
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
        let expected_target = state.exporter.read().await.target_uri();
        let mut events = state.subscribe_events();
        let mut signals = Signals::default();
        signals.settings.target = Some("collector-only".to_string());

        let response = update_settings(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let mut saw_target_revert = false;
        let mut saw_secure_revert = false;
        let mut saw_error = false;
        while let Ok(event) = events.try_recv() {
            match event {
                ServerEvent::Signals(payload) => {
                    if payload
                        .contains(&format!(r#""settings":{{"target":"{}"}}"#, expected_target))
                    {
                        saw_target_revert = true;
                    }
                    if payload.contains(r#""output":{"secure":false}"#) {
                        saw_secure_revert = true;
                    }
                }
                ServerEvent::ExecuteScript(script) => {
                    if script.contains("collector-only") && script.contains("send-capable host") {
                        saw_error = true;
                    }
                }
                _ => {}
            }
        }
        assert!(saw_target_revert, "expected target selection to revert");
        assert!(saw_secure_revert, "expected secure indicator to revert");
        assert!(saw_error, "expected settings error script");
    }

    #[tokio::test]
    async fn service_mode_settings_modal_does_not_touch_local_runtime_features() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, _keystore_path) = setup_env();
        let settings_path = std::env::var_os("ESDIAG_SETTINGS")
            .map(PathBuf::from)
            .expect("settings path env");

        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.runtime_mode_policy = RuntimeModePolicy::new(RuntimeMode::Service);
        let mut events = state.subscribe_events();

        let response = get_modal(State(state)).await.into_response();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let _ = events.try_recv().expect("modal render event");
        assert!(
            !hosts_path.exists(),
            "service mode settings modal should not create hosts.yml"
        );
        assert!(
            !settings_path.exists(),
            "service mode settings modal should not create settings.yml"
        );
    }

    #[tokio::test]
    async fn service_mode_settings_update_requires_iap_header() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.runtime_mode_policy = RuntimeModePolicy::new(RuntimeMode::Service);

        let mut signals = Signals::default();
        signals.settings.kibana_url = Some("https://kibana.example".to_string());

        let response = update_settings(State(state), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn service_mode_settings_update_accepts_iap_header() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.runtime_mode_policy = RuntimeModePolicy::new(RuntimeMode::Service);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            HeaderValue::from_static("accounts.google.com:test@example.com"),
        );

        let mut signals = Signals::default();
        signals.settings.kibana_url = Some("https://kibana.example".to_string());

        let response = update_settings(State(state.clone()), headers, ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            state.kibana_url.read().await.as_str(),
            "https://kibana.example"
        );
    }

    #[tokio::test]
    async fn empty_target_keeps_existing_output_selection() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore_path) = setup_env();

        Settings {
            active_target: Some("stdout".to_string()),
            kibana_url: Some("https://old-kibana.example".to_string()),
        }
        .save()
        .expect("save settings");

        let state = test_server_state();
        let original_target = state.exporter.read().await.target_uri();
        let mut signals = Signals::default();
        signals.settings.target = Some(String::new());
        signals.settings.kibana_url = Some("https://new-kibana.example".to_string());

        let response =
            update_settings(State(state.clone()), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(state.exporter.read().await.target_uri(), original_target);
        let saved = Settings::load().expect("reload settings");
        assert_eq!(saved.active_target.as_deref(), Some("stdout"));
        assert_eq!(
            saved.kibana_url.as_deref(),
            Some("https://new-kibana.example")
        );
    }

    #[tokio::test]
    async fn unchanged_saved_target_does_not_require_unlock_for_kibana_only_update() {
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
                roles: vec![HostRole::Send],
                secret: Some("secure-es".to_string()),
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        Settings {
            active_target: Some("secure-es".to_string()),
            kibana_url: Some("https://old-kibana.example".to_string()),
        }
        .save()
        .expect("save settings");

        let state = test_server_state();
        let mut events = state.subscribe_events();
        let mut signals = Signals::default();
        signals.settings.target = Some("secure-es".to_string());
        signals.settings.kibana_url = Some("https://new-kibana.example".to_string());

        let response =
            update_settings(State(state.clone()), HeaderMap::new(), ReadSignals(signals)).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = Settings::load().expect("reload settings");
        assert_eq!(saved.active_target.as_deref(), Some("secure-es"));
        assert_eq!(
            saved.kibana_url.as_deref(),
            Some("https://new-kibana.example")
        );

        let mut saw_unlock_modal = false;
        while let Ok(event) = events.try_recv() {
            if let ServerEvent::AppendBody(html) = event
                && html.contains("keystore-unlock-modal")
            {
                saw_unlock_modal = true;
            }
        }
        assert!(
            !saw_unlock_modal,
            "unchanged output target should not prompt for unlock"
        );
    }
}
