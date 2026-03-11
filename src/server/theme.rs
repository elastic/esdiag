// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use axum::{
    extract::State,
    http::StatusCode,
    http::{HeaderMap, HeaderValue, header::SET_COOKIE},
    response::IntoResponse,
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
    ReadSignals(signals): ReadSignals<ThemeSignals>,
) -> impl IntoResponse {
    let dark = signals.theme.dark;
    let payload = json!({
        "theme": {
            "dark": dark
        }
    })
    .to_string();
    state.publish_event(signal_event(payload));

    #[cfg(feature = "desktop")]
    {
        // In desktop mode, we need a hard reload so the Tauri window frame reads the new theme_dark cookie
        state.publish_event(execute_script_event("window.location.reload();"));
    }

    let mut headers = HeaderMap::new();
    let dark_cookie = format!(
        "theme_dark={}; Path=/; Max-Age=31536000; SameSite=Lax",
        if dark { "1" } else { "0" }
    );
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&dark_cookie).expect("valid dark cookie"),
    );

    (StatusCode::NO_CONTENT, headers)
}
