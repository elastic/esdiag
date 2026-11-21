// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use axum::{
    extract::State,
    http::StatusCode,
    http::{HeaderMap, HeaderValue, header::SET_COOKIE},
    response::{IntoResponse, Response},
};
use datastar::axum::ReadSignals;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[cfg(feature = "desktop")]
use super::execute_script_event;
use super::{ServerState, signal_event};

#[derive(Debug, Deserialize, Default)]
pub struct ThemeSignals {
    #[serde(default)]
    pub theme: ClientTheme,
}

#[derive(Debug, Deserialize, Default)]
pub struct ClientTheme {
    #[serde(default)]
    pub dark: bool,
}

pub async fn set_theme(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    ReadSignals(signals): ReadSignals<ThemeSignals>,
) -> Response {
    let owner = match state.resolve_user_email(&headers) {
        Ok((_, user)) => user,
        Err(err) if state.server_policy.requires_authentication() => {
            tracing::warn!("Theme update denied: {}", err);
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Err(_) => super::DEFAULT_OWNER.to_string(),
    };
    let dark = signals.theme.dark;
    let payload = json!({
        "theme": {
            "dark": dark
        }
    })
    .to_string();
    state.publish_event_for_owner(&owner, signal_event(payload));

    #[cfg(feature = "desktop")]
    {
        // In desktop mode, we need a hard reload so the Tauri window frame reads the new theme_dark cookie
        state.publish_event_for_owner(&owner, execute_script_event("window.location.reload();"));
    }

    let mut response_headers = HeaderMap::new();
    let dark_cookie = format!(
        "theme_dark={}; Path=/; Max-Age=31536000; SameSite=Lax",
        if dark { "1" } else { "0" }
    );
    response_headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&dark_cookie).expect("valid dark cookie"),
    );

    (StatusCode::NO_CONTENT, response_headers).into_response()
}

#[cfg(test)]
mod tests {
    use super::{ClientTheme, ThemeSignals, set_theme};
    use crate::server::{RuntimeMode, ServerEvent, ServerPolicy, test_server_state};
    use axum::{
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use datastar::axum::ReadSignals;
    use std::sync::Arc;

    #[tokio::test]
    async fn service_mode_theme_update_requires_iap_header() {
        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.server_policy = ServerPolicy::defaults(RuntimeMode::Service);

        let response = set_theme(
            State(state),
            HeaderMap::new(),
            ReadSignals(ThemeSignals {
                theme: ClientTheme { dark: true },
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn service_mode_theme_update_uses_authenticated_owner() {
        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.server_policy = ServerPolicy::defaults(RuntimeMode::Service);
        let mut events = state.subscribe_events();
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            HeaderValue::from_static("accounts.google.com:test@example.com"),
        );

        let response = set_theme(
            State(state),
            headers,
            ReadSignals(ThemeSignals {
                theme: ClientTheme { dark: true },
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let event = events.try_recv().expect("theme event");
        let ServerEvent::Signals { owner, payload, .. } = event else {
            panic!("expected theme signal event");
        };
        assert_eq!(owner, "test@example.com");
        assert!(payload.contains(r#""dark":true"#));
    }
}
