// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use axum::{
    http::{
        HeaderMap, HeaderValue,
        header::{CONTENT_TYPE, SET_COOKIE},
    },
    response::IntoResponse,
};
use datastar::axum::ReadSignals;
use serde::Deserialize;
use serde_json::json;

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

pub async fn set_theme(ReadSignals(signals): ReadSignals<ThemeSignals>) -> impl IntoResponse {
    let dark = signals.theme.dark;
    let payload = json!({
        "theme": {
            "dark": dark
        }
    })
    .to_string();
    let body = datastar::prelude::PatchSignals::new(payload)
        .as_datastar_event()
        .to_string();

    #[cfg(feature = "desktop")]
    {
        // In desktop mode, we need a hard reload so the Tauri window frame reads the new theme_dark cookie
        body.push_str(&datastar::prelude::ExecuteScript::new("window.location.reload();").as_datastar_event().to_string());
    }

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    let dark_cookie = format!(
        "theme_dark={}; Path=/; Max-Age=31536000; SameSite=Lax",
        if dark { "1" } else { "0" }
    );
    headers.append(
        SET_COOKIE,
        HeaderValue::from_str(&dark_cookie).expect("valid dark cookie"),
    );

    (headers, body)
}
