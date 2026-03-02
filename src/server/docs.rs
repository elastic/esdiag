// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::embeds::DocsAssets;
use askama::Template;
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use pulldown_cmark::{Event, Options, Parser, html};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Template)]
#[template(path = "docs.html")]
pub struct DocsTemplate {
    pub toc: Vec<TocEntry>,
    pub current_path: String,
    pub html_content: String,
    // Add layout vars
    pub auth_header: bool,
    pub debug: bool,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub kibana_url: String,
    pub exporter: String,
    pub stats: String,
    pub theme_dark: bool,
}

#[derive(Template)]
#[template(path = "components/docs.html")]
pub struct DocsComponentTemplate {
    pub toc: Vec<TocEntry>,
    pub current_path: String,
    pub html_content: String,
}

pub struct TocEntry {
    pub title: String,
    pub path: String,
    pub level: usize,
    pub is_dir: bool,
}

pub async fn handler_index(
    headers: HeaderMap,
    state: axum::extract::State<std::sync::Arc<crate::server::ServerState>>,
) -> impl IntoResponse {
    handler(headers, state, Path("documentation".to_string())).await
}

pub async fn handler(
    headers: HeaderMap,
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::server::ServerState>>,
    Path(mut path): Path<String>,
) -> impl IntoResponse {
    if path.is_empty() {
        path = "documentation".to_string();
    }

    // Add .md extension if not present
    let file_path = if !path.ends_with(".md") {
        format!("{}.md", path)
    } else {
        path.clone()
    };

    let is_datastar = headers.contains_key("datastar-request");

    match DocsAssets::get(&file_path) {
        Some(content) => {
            let markdown_content = String::from_utf8_lossy(&content.data);

            // Set up pulldown-cmark parser with options
            let mut options = Options::empty();
            options.insert(Options::ENABLE_STRIKETHROUGH);
            options.insert(Options::ENABLE_TABLES);
            options.insert(Options::ENABLE_FOOTNOTES);
            options.insert(Options::ENABLE_TASKLISTS);
            options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
            options.insert(Options::ENABLE_GFM); // GitHub Flavored Markdown

            let parser = Parser::new_ext(&markdown_content, options).map(|event| match event {
                // Do not allow raw HTML from markdown input in rendered docs.
                Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
                _ => event,
            });

            // Write to a new String buffer.
            let mut html_content = String::new();
            html::push_html(&mut html_content, parser);

            let toc = generate_toc();

            // Remove .md suffix for the current_path comparison
            let current_path = if path.ends_with(".md") {
                path[0..path.len() - 3].to_string()
            } else {
                path
            };

            if is_datastar {
                let template = DocsComponentTemplate {
                    toc,
                    current_path,
                    html_content,
                };

                match template.render() {
                    Ok(html) => {
                        let mut response = Html(html).into_response();
                        let headers = response.headers_mut();
                        headers.insert("datastar-selector", "#main-content".parse().unwrap());
                        headers.insert("datastar-mode", "outer".parse().unwrap());
                        response
                    }
                    Err(err) => {
                        log::error!("Template rendering error: {}", err);
                        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
                    }
                }
            } else {
                let (auth_header, user_initial, user_email) =
                    match crate::server::get_user_email(&headers) {
                        (auth_header, Some(email)) => (
                            auth_header,
                            email.chars().next().unwrap_or('_').to_ascii_uppercase(),
                            email,
                        ),
                        _ => (false, '_', "Anonymous".to_string()),
                    };

                let template = DocsTemplate {
                    toc,
                    current_path,
                    html_content,
                    auth_header,
                    debug: log::max_level() == log::Level::Debug,
                    exporter: state.exporter.to_string(),
                    kibana_url: state.kibana_url.clone(),
                    stats: state.get_stats_as_signals().await,
                    user: user_email,
                    user_initial,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    theme_dark: crate::server::get_theme_dark(&headers),
                };

                match template.render() {
                    Ok(html) => Html(html).into_response(),
                    Err(err) => {
                        log::error!("Template rendering error: {}", err);
                        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
                    }
                }
            }
        }
        None => (StatusCode::NOT_FOUND, "Document not found").into_response(),
    }
}

fn generate_toc() -> Vec<TocEntry> {
    let mut sections: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut root_files = Vec::new();

    for file in DocsAssets::iter().map(|p| p.into_owned()) {
        if !file.ends_with(".md") {
            continue;
        }

        let path = PathBuf::from(&file);
        let section = path
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();

        if path.parent().map_or(true, |p| p.as_os_str().is_empty()) {
            root_files.push(file.trim_end_matches(".md").to_string());
        } else {
            let display_path = file.trim_end_matches(".md").to_string();
            sections.entry(section).or_default().push(display_path);
        }
    }

    let mut entries = Vec::new();
    root_files.sort();
    for file in root_files {
        entries.push(TocEntry {
            title: PathBuf::from(file.as_str())
                .file_stem()
                .and_then(|s| s.to_str())
                .map(format_title)
                .unwrap_or_else(|| format_title(file.as_str())),
            path: file,
            level: 0,
            is_dir: false,
        });
    }

    for (section, files) in &mut sections {
        files.sort();
        entries.push(TocEntry {
            title: format_title(section),
            path: String::new(),
            level: 0,
            is_dir: true,
        });

        for file in files {
            let remainder = file
                .strip_prefix(&format!("{section}/"))
                .unwrap_or(file.as_str());
            let title = remainder
                .split('/')
                .map(format_title)
                .collect::<Vec<_>>()
                .join(" / ");

            entries.push(TocEntry {
                title,
                path: file.clone(),
                level: 1,
                is_dir: false,
            });
        }
    }

    entries
}

fn format_title(s: &str) -> String {
    s.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
