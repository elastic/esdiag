// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

// AR - 09/15/2025 - REMOVE `get_user_email` from this line
use super::{ServerState, template};
use askama::Template;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{str::FromStr, sync::Arc};
// AR - 09/15/2025 - ADD this line to import JWT decoding components
use jsonwebtoken::{decode, DecodingKey, Validation};

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

// AR - 09/15/2025 - ADD this struct to deserialize the JWT claims.
#[derive(Debug, Deserialize)]
struct Claims {
    email: String,
}

/// AR 09-16-2025 - added This is the main request handler for the application's index page (`/`).
/// It performs the following key actions:
/// 1.  **Authentication Check**: It inspects the incoming HTTP headers for an
///     `x-pomerium-jwt-assertion` header.
/// 2.  **User Identification**: If the Pomerium JWT is found, it decodes the token
///     (without signature verification) to extract the user's email address. If the
///     header is missing or the token is invalid, it defaults to an "Anonymous" user.
/// 3.  **Template Context Preparation**: It calculates the user's initial from their
///     email and gathers other necessary data from the application's shared state,
///     such as exporter configuration, Kibana URL, and processing statistics.
/// 4.  **HTML Rendering**: It populates the `index.html` template with all the
///     gathered context.
/// 5.  **Response**: It returns the fully rendered HTML page as the response to the
///     client's request.
pub async fn handler(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // This block now contains the final, clean logic for JWT decoding.
    let (auth_header, user_email) = if let Some(jwt_header) = headers.get("x-pomerium-jwt-assertion") {
        let email = jwt_header
            .to_str()
            .ok()
            .and_then(|token| {
                let mut validation = Validation::default();
                validation.validate_exp = false;
                validation.insecure_disable_signature_validation();

                decode::<Claims>(token, &DecodingKey::from_secret(&[]), &validation)
                    .map(|data| data.claims.email)
                    .ok()
            })
            .unwrap_or_else(|| "Anonymous".to_string());
        (true, email)
    } else {
        (false, "Anonymous".to_string())
    };

    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();

    let exporter_target = { state.exporter.read().await.to_string() };
    let index_html = template::Index {
        auth_header,
        debug: log::max_level() == log::Level::Debug,
        exporter: exporter_target,
        kibana_url: state.kibana_url.clone(),
        key_id: params.key_id,
        link_id: params.link_id,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let index_html = match index_html.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(index_html)
}