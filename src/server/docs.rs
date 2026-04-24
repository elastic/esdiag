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
use regex::Regex;
use std::{collections::BTreeMap, collections::HashSet, path::PathBuf, sync::OnceLock};

#[derive(Template)]
#[template(path = "docs.html")]
pub struct DocsTemplate {
    pub nav_root_items: Vec<DocsNavLink>,
    pub nav_sections: Vec<DocsNavSection>,
    pub current_path: String,
    pub html_content: String,
    // Add layout vars
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub kibana_url: String,
    pub stats: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub show_advanced: bool,
    pub show_job_builder: bool,
    pub can_use_keystore: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
}

#[derive(Template)]
#[template(path = "components/docs.html")]
pub struct DocsComponentTemplate {
    pub nav_root_items: Vec<DocsNavLink>,
    pub nav_sections: Vec<DocsNavSection>,
    pub current_path: String,
    pub html_content: String,
}

pub struct DocsNavLink {
    pub title: String,
    pub path: String,
    pub is_active: bool,
}

pub struct DocsNavSection {
    pub id: String,
    pub title: String,
    pub open: bool,
    pub items: Vec<DocsNavLink>,
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
            html_content = inject_heading_ids(&html_content);

            let toc = generate_toc();

            // Remove .md suffix for the current_path comparison
            let current_path = if path.ends_with(".md") {
                path[0..path.len() - 3].to_string()
            } else {
                path
            };
            let (nav_root_items, nav_sections) = build_nav(&toc, &current_path);

            if is_datastar {
                let template = DocsComponentTemplate {
                    nav_root_items,
                    nav_sections,
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
                        tracing::error!("Template rendering error: {}", err);
                        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
                    }
                }
            } else {
                let (auth_header, user_email) = match state.resolve_user_email(&headers) {
                    Ok(result) => result,
                    Err(err) => {
                        return (StatusCode::UNAUTHORIZED, format!("Unauthorized: {err}")).into_response();
                    }
                };
                let user_initial = user_email.chars().next().unwrap_or('_').to_ascii_uppercase();
                let keystore_state = state.keystore_page_state().await;

                let template = DocsTemplate {
                    nav_root_items,
                    nav_sections,
                    current_path,
                    html_content,
                    auth_header,
                    debug: tracing::enabled!(tracing::Level::DEBUG),
                    desktop: cfg!(feature = "desktop"),
                    kibana_url: state.kibana_url.read().await.clone(),
                    stats: state.get_stats_as_signals().await,
                    user: user_email,
                    user_initial,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    theme_dark: crate::server::get_theme_dark(&headers),
                    runtime_mode: state.runtime_mode.to_string(),
                    show_advanced: state.server_policy.allows_advanced(),
                    show_job_builder: state.server_policy.allows_job_builder(),
                    can_use_keystore: keystore_state.can_use_keystore,
                    keystore_locked: keystore_state.locked,
                    keystore_lock_time: keystore_state.lock_time,
                };

                match template.render() {
                    Ok(html) => Html(html).into_response(),
                    Err(err) => {
                        tracing::error!("Template rendering error: {}", err);
                        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
                    }
                }
            }
        }
        None => (StatusCode::NOT_FOUND, "Document not found").into_response(),
    }
}

fn build_nav(entries: &[TocEntry], current_path: &str) -> (Vec<DocsNavLink>, Vec<DocsNavSection>) {
    let mut root_items = Vec::new();
    let mut sections = Vec::new();
    let mut current_section: Option<DocsNavSection> = None;

    for entry in entries {
        if entry.is_dir {
            if let Some(section) = current_section.take() {
                sections.push(section);
            }
            current_section = Some(DocsNavSection {
                id: slugify(&entry.title),
                title: entry.title.clone(),
                open: false,
                items: Vec::new(),
            });
            continue;
        }

        let link = DocsNavLink {
            title: entry.title.clone(),
            path: entry.path.clone(),
            is_active: current_path == entry.path,
        };

        if entry.level == 0 {
            if let Some(section) = current_section.take() {
                sections.push(section);
            }
            root_items.push(link);
        } else if let Some(section) = current_section.as_mut() {
            if link.is_active {
                section.open = true;
            }
            section.items.push(link);
        } else {
            root_items.push(link);
        }
    }

    if let Some(section) = current_section.take() {
        sections.push(section);
    }

    (root_items, sections)
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

        if path.parent().is_none_or(|p| p.as_os_str().is_empty()) {
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
            let remainder = file.strip_prefix(&format!("{section}/")).unwrap_or(file.as_str());
            let title = remainder.split('/').map(format_title).collect::<Vec<_>>().join(" / ");

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

fn slugify(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn extract_existing_ids(html: &str) -> HashSet<String> {
    static ID_ATTR_RE: OnceLock<Regex> = OnceLock::new();
    let re = ID_ATTR_RE
        .get_or_init(|| Regex::new(r#"(?i)\bid\s*=\s*"([^"]+)""#).expect("id attribute regex should compile"));
    re.captures_iter(html)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn strip_html_tags(input: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let re = TAG_RE.get_or_init(|| Regex::new(r"(?s)<[^>]*>").expect("tag regex should compile"));
    re.replace_all(input, " ").into_owned()
}

fn next_unique_id(base: &str, used_ids: &mut HashSet<String>) -> String {
    let root = if base.is_empty() { "section" } else { base };
    if used_ids.insert(root.to_string()) {
        return root.to_string();
    }

    let mut suffix = 2usize;
    loop {
        let candidate = format!("{root}-{suffix}");
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn inject_heading_ids(html: &str) -> String {
    static HEADING_RE: OnceLock<Regex> = OnceLock::new();
    static HAS_ID_RE: OnceLock<Regex> = OnceLock::new();
    let heading_re = HEADING_RE
        .get_or_init(|| Regex::new(r#"(?is)<h([1-3])([^>]*)>(.*?)</h([1-3])>"#).expect("heading regex should compile"));
    let has_id_re =
        HAS_ID_RE.get_or_init(|| Regex::new(r#"(?i)\bid\s*="#).expect("heading id presence regex should compile"));

    let mut used_ids = extract_existing_ids(html);
    heading_re
        .replace_all(html, |caps: &regex::Captures<'_>| {
            let level = caps.get(1).map_or("1", |m| m.as_str());
            let attrs = caps.get(2).map_or("", |m| m.as_str());
            let content = caps.get(3).map_or("", |m| m.as_str());
            let closing_level = caps.get(4).map_or(level, |m| m.as_str());

            if closing_level != level {
                return caps.get(0).map_or_else(String::new, |m| m.as_str().to_string());
            }

            if has_id_re.is_match(attrs) {
                return caps.get(0).map_or_else(String::new, |m| m.as_str().to_string());
            }

            let label_text = strip_html_tags(content);
            let base = slugify(label_text.trim());
            let id = next_unique_id(&base, &mut used_ids);

            format!(r#"<h{level}{attrs} id="{id}">{content}</h{level}>"#)
        })
        .into_owned()
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::handler_index;
    use crate::server::{RuntimeMode, ServerPolicy, test_server_state};
    use axum::{
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use std::{sync::Arc, sync::Mutex};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        let hosts_path = config_dir.join("hosts.yml");
        let settings_path = config_dir.join("settings.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
            std::env::set_var("ESDIAG_SETTINGS", &settings_path);
        }
        (tmp, hosts_path, settings_path)
    }

    #[tokio::test]
    async fn service_mode_docs_does_not_touch_local_runtime_features() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, settings_path) = setup_env();

        let mut state = test_server_state();
        let state_mut = Arc::get_mut(&mut state).expect("unique state");
        state_mut.runtime_mode = RuntimeMode::Service;
        state_mut.server_policy = ServerPolicy::new(RuntimeMode::Service).expect("test server policy");

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Goog-Authenticated-User-Email",
            HeaderValue::from_static("accounts.google.com:test@example.com"),
        );

        let response = handler_index(headers, State(state)).await.into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!hosts_path.exists(), "service mode docs should not create hosts.yml");
        assert!(
            !settings_path.exists(),
            "service mode docs should not create settings.yml"
        );
    }
}
