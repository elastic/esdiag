// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
};
use crate::embeds::{Assets, ServerAssets};

fn serve_server_asset(path: &str, content_type: &'static str) -> impl IntoResponse {
    match ServerAssets::get(path) {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, content_type)], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn datastar() -> impl IntoResponse {
    serve_server_asset("datastar.js", "text/javascript")
}

pub async fn datastar_map() -> impl IntoResponse {
    serve_server_asset("datastar.js.map", "application/json")
}

pub async fn logo() -> impl IntoResponse {
    serve_server_asset("esdiag.svg", "image/svg+xml")
}

pub async fn style() -> impl IntoResponse {
    serve_server_asset("style.css", "text/css")
}


pub async fn prism() -> impl IntoResponse {
    match Assets::get("prism.js") {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, "text/javascript")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn prism_bash() -> impl IntoResponse {
    match Assets::get("prism-bash.js") {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, "text/javascript")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn prism_json() -> impl IntoResponse {
    match Assets::get("prism-json.js") {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, "text/javascript")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn prism_json5() -> impl IntoResponse {
    match Assets::get("prism-json5.js") {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, "text/javascript")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn prism_css() -> impl IntoResponse {
    match Assets::get("prism.css") {
        Some(content) => {
            let body = content.data.into_owned();
            ([(header::CONTENT_TYPE, "text/css")], body).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
