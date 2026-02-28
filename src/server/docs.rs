// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use askama::Template;
use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
};
use crate::embeds::DocsAssets;
use std::path::PathBuf;

#[derive(Template)]
#[template(path = "docs.html")]
pub struct DocsTemplate {
    pub toc: Vec<TocEntry>,
    pub current_path: String,
    pub markdown_content: String,
    // Add layout vars
    pub auth_header: bool,
    pub debug: bool,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub kibana_url: String,
    pub exporter: String,
    pub stats: String,
}

pub struct TocEntry {
    pub title: String,
    pub path: String,
    pub level: usize,
    pub is_dir: bool,
}

pub async fn handler(
    headers: HeaderMap, 
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::server::ServerState>>,
    Path(mut path): Path<String>
) -> impl IntoResponse {
    if path.is_empty() {
        path = "index".to_string();
    }
    
    // Add .md extension if not present
    let file_path = if !path.ends_with(".md") {
        format!("{}.md", path)
    } else {
        path.clone()
    };

    match DocsAssets::get(&file_path) {
        Some(content) => {
            let markdown_content = String::from_utf8_lossy(&content.data).into_owned();
            let toc = generate_toc();
            
            // Remove .md suffix for the current_path comparison
            let current_path = if path.ends_with(".md") {
                path[0..path.len()-3].to_string()
            } else {
                path
            };

            let (auth_header, user_initial, user_email) = match crate::server::get_user_email(&headers) {
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
                markdown_content,
                auth_header,
                debug: log::max_level() == log::Level::Debug,
                exporter: state.exporter.to_string(),
                kibana_url: state.kibana_url.clone(),
                stats: state.get_stats_as_signals().await,
                user: user_email,
                user_initial,
                version: env!("CARGO_PKG_VERSION").to_string(),
            };
            
            match template.render() {
                Ok(html) => Html(html).into_response(),
                Err(err) => {
                    log::error!("Template rendering error: {}", err);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
                }
            }
        }
        None => (StatusCode::NOT_FOUND, "Document not found").into_response(),
    }
}

fn generate_toc() -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut files: Vec<_> = DocsAssets::iter().map(|p| p.into_owned()).collect();
    
    // Simple sort to ensure stable ordering
    files.sort();

    let mut current_dir = String::new();

    for file in files {
        let path = PathBuf::from(&file);
        
        // Handle directories
        if let Some(parent) = path.parent() {
            let parent_str = parent.to_string_lossy().into_owned();
            if !parent_str.is_empty() && parent_str != current_dir {
                current_dir = parent_str.clone();
                entries.push(TocEntry {
                    title: format_title(&current_dir),
                    path: String::new(),
                    level: 0,
                    is_dir: true,
                });
            }
        } else if !current_dir.is_empty() {
            current_dir = String::new();
        }

        // Format title from filename
        let title = format_title(path.file_stem().unwrap_or_default().to_str().unwrap_or(&file));
        
        // Path without .md
        let display_path = if file.ends_with(".md") {
            file[0..file.len()-3].to_string()
        } else {
            file.clone()
        };

        let level = if path.parent().map_or(true, |p| p.as_os_str().is_empty()) {
            0
        } else {
            1
        };

        entries.push(TocEntry {
            title,
            path: display_path,
            level,
            is_dir: false,
        });
    }

    entries
}

fn format_title(s: &str) -> String {
    // Capitalize and replace hyphens/underscores with spaces
    let mut title = s.replace(['-', '_'], " ");
    if let Some(first) = title.chars().next() {
        title = format!("{}{}", first.to_uppercase(), &title[first.len_utf8()..]);
    }
    title
}
