// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ServerState, get_theme_dark, template};
use crate::data::{
    HostRole, KnownHost, Product, SavedJob, Settings, keystore_exists, load_saved_jobs,
};
use crate::exporter::Exporter;
use crate::processor::api::ApiResolver;
use askama::Template;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{path::PathBuf, str::FromStr, sync::Arc};

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
            tracing::warn!("Authentication header validation failed: {err}");
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
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();
    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let output_secure = if allows_local_runtime_features {
        let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
        let send_hosts: Vec<String> = hosts_by_name
            .iter()
            .filter(|(_, h)| h.has_role(HostRole::Send))
            .map(|(name, _)| name.clone())
            .collect();
        let exporter = state.exporter.read().await.clone();
        let preferred_target = Settings::load()
            .ok()
            .and_then(|settings| settings.active_target);
        let (_output_options, selected_output, _label) = template::build_footer_output_context(
            &hosts_by_name,
            &send_hosts,
            &exporter,
            preferred_target.as_deref(),
        );
        template::active_output_requires_keystore(
            &hosts_by_name,
            &send_hosts,
            &selected_output,
            &exporter,
        )
    } else {
        false
    };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let page = template::Index {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
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
        can_use_keystore,
        output_secure,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

pub async fn workflow_page(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("Authentication header validation failed: {err}");
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
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();

    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let exporter = { state.exporter.read().await.clone() };
    let send_defaults = classify_configured_exporter(&exporter);
    let workflow_hosts = workflow_host_options(&state);
    let default_save_dir = default_downloads_dir().display().to_string();
    let process_options_json =
        serde_json::to_string(&ApiResolver::processing_catalog().unwrap_or_default())
            .unwrap_or_else(|_| "{}".to_string());
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);
    let page = template::Workflow {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        collect_hosts: workflow_hosts.collect_hosts,
        collect_secure_hosts_json: serde_json::to_string(&workflow_hosts.collect_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        configured_local_path: send_defaults.local_path,
        configured_remote_target: send_defaults.remote_target,
        default_save_dir,
        initial_send_mode: send_defaults.mode,
        initial_local_target: send_defaults.local_target,
        initial_remote_target: send_defaults.remote_target_default,
        kibana_url,
        key_id: params.key_id,
        link_id: params.link_id,
        process_options_json,
        send_secure_hosts_json: serde_json::to_string(&workflow_hosts.send_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        send_local_hosts: workflow_hosts.send_local_hosts,
        send_remote_hosts: workflow_hosts.send_remote_hosts,
        upload_id: params.upload_id,
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

pub async fn jobs_page(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    build_jobs_page(state, None, Some(params), headers).await
}

pub async fn jobs_page_with_saved_job(
    state: Arc<ServerState>,
    name: String,
    headers: HeaderMap,
) -> Response {
    build_jobs_page(state, Some(name), None, headers)
        .await
        .into_response()
}

async fn build_jobs_page(
    state: Arc<ServerState>,
    saved_job_name: Option<String>,
    params: Option<Params>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (auth_header, user_email) = match state.resolve_user_email(&headers) {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!("Authentication header validation failed: {err}");
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
    let user_initial = user_email
        .chars()
        .next()
        .unwrap_or('_')
        .to_ascii_uppercase();

    let allows_local_runtime_features = state.runtime_mode_policy.allows_local_runtime_features();
    let can_use_keystore = cfg!(feature = "keystore") && allows_local_runtime_features;
    let exporter = { state.exporter.read().await.clone() };
    let send_defaults = classify_configured_exporter(&exporter);
    let workflow_hosts = workflow_host_options(&state);
    let default_save_dir = default_downloads_dir().display().to_string();
    let process_options_json =
        serde_json::to_string(&ApiResolver::processing_catalog().unwrap_or_default())
            .unwrap_or_else(|_| "{}".to_string());
    let theme_dark = get_theme_dark(&headers);
    let kibana_url = { state.kibana_url.read().await.clone() };
    let (keystore_locked, keystore_lock_time) = if can_use_keystore {
        state.keystore_status_for(&user_email).await
    } else {
        (true, 0)
    };
    let show_keystore_bootstrap = can_use_keystore && !keystore_exists().unwrap_or(false);

    // Resolve saved job if a name was provided
    let (saved_job, job_not_found, job_load_error) = if let Some(ref name) = saved_job_name {
        match load_saved_jobs() {
            Ok(jobs) => match jobs.get(name).cloned() {
                Some(job) => (Some(job), false, None),
                None => (None, true, None),
            },
            Err(err) => {
                tracing::error!("Failed to load saved jobs: {err}");
                (None, false, Some("Failed to load saved jobs".to_string()))
            }
        }
    } else {
        (None, false, None)
    };

    let stale_host = saved_job.as_ref().is_some_and(|job| {
        let h = &job.workflow.collect.known_host;
        !h.is_empty() && !workflow_hosts.collect_hosts.contains(h)
    });
    let hide_saved_job = job_not_found || job_load_error.is_some();

    let saved = SavedJobDefaults::from_job(saved_job.as_ref(), &send_defaults, &default_save_dir);

    let message = if let Some(err) = job_load_error {
        err
    } else if job_not_found {
        format!(
            "Job '{}' not found",
            saved_job_name.as_deref().unwrap_or("")
        )
    } else if stale_host {
        format!(
            "Warning: host '{}' referenced by job '{}' is no longer configured",
            saved.known_host,
            saved_job_name.as_deref().unwrap_or("")
        )
    } else {
        String::new()
    };

    let page = template::Jobs {
        auth_header,
        debug: tracing::enabled!(tracing::Level::DEBUG),
        desktop: cfg!(feature = "desktop"),
        collect_hosts: workflow_hosts.collect_hosts,
        collect_secure_hosts_json: serde_json::to_string(&workflow_hosts.collect_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        configured_local_path: send_defaults.local_path,
        configured_remote_target: send_defaults.remote_target,
        default_save_dir,
        kibana_url,
        key_id: params.as_ref().and_then(|p| p.key_id),
        link_id: params.as_ref().and_then(|p| p.link_id),
        process_options_json,
        send_secure_hosts_json: serde_json::to_string(&workflow_hosts.send_secure_hosts)
            .unwrap_or_else(|_| "[]".to_string()),
        send_local_hosts: workflow_hosts.send_local_hosts,
        send_remote_hosts: workflow_hosts.send_remote_hosts,
        upload_id: params.as_ref().and_then(|p| p.upload_id),
        stats: state.get_stats_as_signals().await,
        user: user_email,
        user_initial,
        version: env!("CARGO_PKG_VERSION").to_string(),
        theme_dark,
        runtime_mode: state.runtime_mode.to_string(),
        can_use_keystore,
        keystore_locked,
        keystore_lock_time,
        show_keystore_bootstrap,
        saved_job_name: if hide_saved_job { None } else { saved_job_name },
        saved_collect_mode: saved.collect_mode,
        saved_collect_source: saved.collect_source,
        saved_known_host: saved.known_host,
        saved_diagnostic_type: saved.diagnostic_type,
        saved_collect_save: saved.collect_save,
        saved_save_dir: saved.save_dir,
        saved_process_mode: saved.process_mode,
        saved_process_enabled: saved.process_enabled,
        saved_process_product: saved.process_product,
        saved_process_diagnostic_type: saved.process_diagnostic_type,
        saved_process_selected: saved.process_selected,
        saved_send_mode: saved.send_mode,
        saved_remote_target: saved.remote_target,
        saved_local_target: saved.local_target,
        saved_local_directory: saved.local_directory,
        saved_user: saved.user,
        saved_account: saved.account,
        saved_case_number: saved.case_number,
        saved_opportunity: saved.opportunity,
        saved_stale_host: stale_host,
        saved_message: message,
    };

    let html = match page.render() {
        Ok(html) => html,
        Err(err) => format!(
            "<html><body><h1>Internal Server Error</h1><p>{}</p></body></html>",
            err
        ),
    };

    Html(html).into_response()
}

struct SavedJobDefaults {
    collect_mode: String,
    collect_source: String,
    known_host: String,
    diagnostic_type: String,
    collect_save: bool,
    save_dir: String,
    process_mode: String,
    process_enabled: bool,
    process_product: String,
    process_diagnostic_type: String,
    process_selected: String,
    send_mode: String,
    remote_target: String,
    local_target: String,
    local_directory: String,
    user: String,
    account: String,
    case_number: String,
    opportunity: String,
}

impl SavedJobDefaults {
    fn from_job(
        job: Option<&SavedJob>,
        send_defaults: &SendDefaults,
        default_save_dir: &str,
    ) -> Self {
        if let Some(job) = job {
            Self {
                collect_mode: serde_json::to_string(&job.workflow.collect.mode)
                    .unwrap_or_else(|_| "\"upload\"".to_string()),
                collect_source: serde_json::to_string(&job.workflow.collect.source)
                    .unwrap_or_else(|_| "\"upload-file\"".to_string()),
                known_host: job.workflow.collect.known_host.clone(),
                diagnostic_type: job.workflow.collect.diagnostic_type.clone(),
                collect_save: job.workflow.collect.save,
                save_dir: if job.workflow.collect.save_dir.is_empty() {
                    default_save_dir.to_string()
                } else {
                    job.workflow.collect.save_dir.clone()
                },
                process_mode: serde_json::to_string(&job.workflow.process.mode)
                    .unwrap_or_else(|_| "\"process\"".to_string()),
                process_enabled: job.workflow.process.enabled,
                process_product: job.workflow.process.product.clone(),
                process_diagnostic_type: job.workflow.process.diagnostic_type.clone(),
                process_selected: job.workflow.process.selected.clone(),
                send_mode: serde_json::to_string(&job.workflow.send.mode)
                    .unwrap_or_else(|_| format!("\"{}\"", send_defaults.mode)),
                remote_target: job.workflow.send.remote_target.clone(),
                local_target: job.workflow.send.local_target.clone(),
                local_directory: if job.workflow.send.local_directory.is_empty() {
                    default_save_dir.to_string()
                } else {
                    job.workflow.send.local_directory.clone()
                },
                user: job.identifiers.user.clone().unwrap_or_default(),
                account: job.identifiers.account.clone().unwrap_or_default(),
                case_number: job.identifiers.case_number.clone().unwrap_or_default(),
                opportunity: job.identifiers.opportunity.clone().unwrap_or_default(),
            }
        } else {
            Self {
                collect_mode: "\"upload\"".to_string(),
                collect_source: "\"upload-file\"".to_string(),
                known_host: String::new(),
                diagnostic_type: "standard".to_string(),
                collect_save: false,
                save_dir: default_save_dir.to_string(),
                process_mode: "\"process\"".to_string(),
                process_enabled: true,
                process_product: "elasticsearch".to_string(),
                process_diagnostic_type: "standard".to_string(),
                process_selected: String::new(),
                send_mode: format!("\"{}\"", send_defaults.mode),
                remote_target: send_defaults.remote_target_default.clone(),
                local_target: send_defaults.local_target.clone(),
                local_directory: String::new(),
                user: String::new(),
                account: String::new(),
                case_number: String::new(),
                opportunity: String::new(),
            }
        }
    }
}

struct SendDefaults {
    mode: String,
    local_path: String,
    local_target: String,
    remote_target: String,
    remote_target_default: String,
}

struct WorkflowHostOptions {
    collect_hosts: Vec<String>,
    collect_secure_hosts: Vec<String>,
    send_remote_hosts: Vec<String>,
    send_local_hosts: Vec<String>,
    send_secure_hosts: Vec<String>,
}

fn classify_configured_exporter(exporter: &Exporter) -> SendDefaults {
    let target_uri = exporter.target_uri();
    match exporter {
        Exporter::Elasticsearch(_) => SendDefaults {
            mode: "remote".to_string(),
            local_path: String::new(),
            local_target: String::new(),
            remote_target: target_uri.clone(),
            remote_target_default: target_uri,
        },
        Exporter::Directory(_) | Exporter::File(_) => SendDefaults {
            mode: "local".to_string(),
            local_path: target_uri,
            local_target: "directory".to_string(),
            remote_target: String::new(),
            remote_target_default: String::new(),
        },
        Exporter::Archive(_) | Exporter::Stream(_) => SendDefaults {
            mode: "remote".to_string(),
            local_path: String::new(),
            local_target: String::new(),
            remote_target: target_uri.clone(),
            remote_target_default: target_uri,
        },
    }
}

fn workflow_host_options(state: &Arc<ServerState>) -> WorkflowHostOptions {
    if !state.runtime_mode_policy.allows_host_management() {
        return WorkflowHostOptions {
            collect_hosts: Vec::new(),
            collect_secure_hosts: Vec::new(),
            send_remote_hosts: Vec::new(),
            send_local_hosts: Vec::new(),
            send_secure_hosts: Vec::new(),
        };
    }

    let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
    let mut collect_hosts = Vec::new();
    let mut collect_secure_hosts = Vec::new();
    let mut send_remote_hosts = Vec::new();
    let mut send_local_hosts = Vec::new();
    let mut send_secure_hosts = Vec::new();

    for (name, host) in hosts_by_name {
        if host.has_role(HostRole::Collect) {
            collect_hosts.push(name.clone());
            if host.requires_keystore_secret() {
                collect_secure_hosts.push(name.clone());
            }
        }

        if host.has_role(HostRole::Send) && host.app() == &Product::Elasticsearch {
            send_remote_hosts.push(name.clone());
            if host.requires_keystore_secret() {
                send_secure_hosts.push(name.clone());
            }
            if host.get_url().host_str().is_some_and(is_local_host) {
                send_local_hosts.push(name.clone());
            }
        }
    }

    WorkflowHostOptions {
        collect_hosts,
        collect_secure_hosts,
        send_remote_hosts,
        send_local_hosts,
        send_secure_hosts,
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1")
}

fn default_downloads_dir() -> PathBuf {
    let home_dir = match std::env::consts::OS {
        "windows" => std::env::var("USERPROFILE").unwrap_or_default(),
        "linux" | "macos" => std::env::var("HOME").unwrap_or_default(),
        _ => String::new(),
    };

    let home_path = PathBuf::from(home_dir);
    if home_path.as_os_str().is_empty() {
        PathBuf::from("Downloads")
    } else {
        home_path.join("Downloads")
    }
}

#[cfg(test)]
mod tests {
    use super::workflow_host_options;
    use crate::{
        data::{HostRole, KnownHost, KnownHostBuilder, Product},
        server::test_server_state,
    };
    use std::collections::BTreeMap;
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_hosts() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let hosts_path = config_dir.join("hosts.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "es-remote".to_string(),
            KnownHostBuilder::new(Url::parse("https://es.example.com:9200").expect("es url"))
                .product(Product::Elasticsearch)
                .roles(vec![HostRole::Send])
                .build()
                .expect("es host"),
        );
        hosts.insert(
            "es-local".to_string(),
            KnownHostBuilder::new(Url::parse("http://localhost:9200").expect("local es url"))
                .product(Product::Elasticsearch)
                .roles(vec![HostRole::Send])
                .build()
                .expect("local es host"),
        );
        hosts.insert(
            "kb-collect".to_string(),
            KnownHostBuilder::new(Url::parse("https://kb.example.com:5601").expect("kb url"))
                .product(Product::Kibana)
                .roles(vec![HostRole::Collect])
                .build()
                .expect("kb host"),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        tmp
    }

    #[test]
    fn workflow_host_options_only_offer_elasticsearch_send_hosts() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_hosts();
        let state = test_server_state();

        let options = workflow_host_options(&state);

        assert_eq!(options.send_remote_hosts.len(), 2);
        assert!(options.send_remote_hosts.contains(&"es-remote".to_string()));
        assert!(options.send_remote_hosts.contains(&"es-local".to_string()));
        assert_eq!(options.send_local_hosts, vec!["es-local".to_string()]);
        assert!(options.collect_hosts.contains(&"kb-collect".to_string()));
        assert!(
            !options
                .send_remote_hosts
                .contains(&"kb-collect".to_string())
        );
        assert!(!options.send_local_hosts.contains(&"kb-collect".to_string()));
        assert!(
            !options
                .send_secure_hosts
                .contains(&"kb-collect".to_string())
        );
    }
}
