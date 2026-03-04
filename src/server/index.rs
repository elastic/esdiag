// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerState, get_theme_dark, template};
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
    let user_initial = user_email.chars().next().unwrap_or('_').to_ascii_uppercase();

    let exporter_target = { state.exporter.read().await.to_string() };
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let index_html = template::Index {
        auth_header,
        debug: log::max_level() >= log::LevelFilter::Debug,
        desktop: cfg!(feature = "desktop"),
        can_configure_output: state.runtime_mode_policy.allows_exporter_updates(),
        exporter: exporter_target,
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
