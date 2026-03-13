// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerState, get_theme_dark, template};
use crate::data::{HostRole, KnownHost, keystore_exists};
use askama::Template;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{str::FromStr, sync::Arc};

#[allow(dead_code)] // Needed when deserializing signals to modify selected tab in Web UI
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    FileUpload,
    ServiceLink,
    ApiKey,
}

impl std::fmt::Display for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tab::FileUpload => write!(f, "file_upload"),
            Tab::ServiceLink => write!(f, "service_link"),
            Tab::ApiKey => write!(f, "api_key"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Params {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    key_id: Option<u64>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    link_id: Option<u64>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    upload_id: Option<u64>,
}

/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

pub async fn handler(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            log::warn!("Authentication header validation failed: {err}");
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

    let allows_local_artifacts = state.runtime_mode_policy.allows_local_artifacts();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_artifacts;
    let exporter = { state.exporter.read().await.clone() };
    let send_hosts = if allows_local_artifacts {
        KnownHost::list_by_role(HostRole::Send).unwrap_or_default()
    } else {
        vec![]
    };
    let preferred_target = if allows_local_artifacts {
        crate::data::Settings::load()
            .ok()
            .and_then(|settings| settings.active_target)
    } else {
        None
    };
    let (output_options, selected_output, exporter_label) =
        template::build_footer_output_context(&send_hosts, &exporter, preferred_target.as_deref());
    let active_output_secure =
        template::active_output_requires_keystore(&send_hosts, &selected_output, &exporter);
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let index_html = template::Index {
        auth_header,
        debug: log::max_level() >= log::LevelFilter::Debug,
        desktop: cfg!(feature = "desktop"),
        can_configure_output: state.runtime_mode_policy.allows_exporter_updates(),
        output_options,
        selected_output,
        exporter_label,
        active_output_secure,
        kibana_url,
        key_id: params.key_id,
        link_id: params.link_id,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let index_html = match index_html.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(index_html).into_response()
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::{Params, handler};
    use crate::{
        exporter::Exporter,
        server::{RuntimeMode, RuntimeModePolicy, test_server_state},
    };
    use axum::{
        extract::{Query, State},
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use std::{sync::Arc, sync::Mutex};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        let hosts_path = config_dir.join("hosts.yml");
        let settings_path = config_dir.join("settings.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::set_var("ESDIAG_SETTINGS", &settings_path);
        }
        (tmp, hosts_path, settings_path)
    }

    #[tokio::test]
    async fn service_mode_index_does_not_touch_local_artifacts() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, settings_path) = setup_env();

        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.runtime_mode_policy = RuntimeModePolicy::new(RuntimeMode::Service);
        *state.exporter.write().await = Exporter::default();

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            HeaderValue::from_static("accounts.google.com:test@example.com"),
        );

        let response = handler(
            State(state),
            Query(Params {
                key_id: None,
                link_id: None,
                upload_id: None,
            }),
            headers,
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            !hosts_path.exists(),
            "service mode index should not create hosts.yml"
        );
        assert!(
            !settings_path.exists(),
            "service mode index should not create settings.yml"
        );
    }
}
