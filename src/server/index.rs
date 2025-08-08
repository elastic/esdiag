use super::{ServerState, get_user_email, template};
use askama::Template;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{str::FromStr, sync::Arc};

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
    link_id: Option<u64>,
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
    let (auth_header, user_initial, user_email) = match get_user_email(&headers) {
        (auth_header, Some(email)) => (
            auth_header,
            email.chars().next().unwrap_or('_').to_ascii_uppercase(),
            email,
        ),
        _ => (false, '_', "Anonymous".to_string()),
    };

    let exporter_target = { state.exporter.read().await.to_string() };
    let index_html = template::Index {
        auth_header,
        debug: log::max_level() == log::Level::Debug,
        exporter: exporter_target,
        kibana_url: state.kibana.clone(),
        link_id: params.link_id,
        stats: state.get_stats_as_signals().await,
        upload_id: params.upload_id,
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
