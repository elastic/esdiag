use super::{ServerState, Signals, get_iap_email, template};
use askama::Template;
use async_stream::stream;
use axum::{
    extract::Path,
    http::HeaderMap,
    response::{Html, IntoResponse, Sse},
};
use datastar::{
    axum::ReadSignals,
    prelude::{PatchElements, PatchSignals},
};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc};

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

pub async fn handler(headers: HeaderMap, state: Arc<ServerState>) -> impl IntoResponse {
    let (user_initial, user_email) = match get_iap_email(&headers) {
        Some(email) => (email.chars().next().unwrap_or('_'), email),
        None => ('_', "Anonymous".to_string()),
    };
    let exporter_target = { state.exporter.read().await.to_string() };
    let index_html = template::Index {
        exporter: exporter_target,
        kibana_url: state.kibana.clone(),
        debug: log::max_level() == log::Level::Debug,
        user_initial,
        user: user_email,
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

pub async fn tab_handler(
    Path(tab): Path<Tab>,
    ReadSignals(_signals): ReadSignals<Signals>,
) -> impl IntoResponse {
    let signals = format!(r#"{{"tab":"{tab}"}}"#);
    let elements = match tab {
        Tab::FileUpload => FileUploadTab {}.render(),
        Tab::ServiceLink => ServiceLinkTab {}.render(),
        Tab::ApiKey => ApiKeyTab {}.render(),
    };

    let elements = elements.unwrap_or_else(|err| {
        format!(
            r#"<div id="tab-content" class="tab-content"><h3>Internal Server Error:</h3><p>{}</p></div>"#,
            err
        )
    });

    Sse::new(stream! {
        let patch = PatchSignals::new(signals);
        let sse_event = patch.write_as_axum_sse_event();
        yield Ok::<_, Infallible>(sse_event);
        let patch = PatchElements::new(elements);
        let sse_event = patch.write_as_axum_sse_event();
        yield Ok::<_, Infallible>(sse_event);
    })
}

#[derive(Template)]
#[template(path = "index/tab/file_upload.html")]
pub struct FileUploadTab {}

#[derive(Template)]
#[template(path = "index/tab/service_link.html")]
pub struct ServiceLinkTab {}

#[derive(Template)]
#[template(path = "index/tab/api_key.html")]
pub struct ApiKeyTab {}
