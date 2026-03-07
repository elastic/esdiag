// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::embeds::ServerAssets;
use axum::{
    http::{StatusCode, header},
    response::IntoResponse,
};

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

pub async fn theme_borealis() -> impl IntoResponse {
    serve_server_asset("theme-borealis.css", "text/css")
}

pub async fn prism() -> impl IntoResponse {
    serve_server_asset("prism.js", "text/javascript")
}

pub async fn prism_bash() -> impl IntoResponse {
    serve_server_asset("prism-bash.js", "text/javascript")
}

pub async fn prism_json() -> impl IntoResponse {
    serve_server_asset("prism-json.js", "text/javascript")
}

pub async fn prism_json5() -> impl IntoResponse {
    serve_server_asset("prism-json5.js", "text/javascript")
}

pub async fn prism_rust() -> impl IntoResponse {
    serve_server_asset("prism-rust.js", "text/javascript")
}

pub async fn prism_css() -> impl IntoResponse {
    serve_server_asset("prism.css", "text/css")
}

pub async fn documentation_outline() -> impl IntoResponse {
    serve_server_asset("documentation-outline.js", "text/javascript")
}

pub async fn document_outline() -> impl IntoResponse {
    serve_server_asset("documentation-outline.js", "text/javascript")
}
