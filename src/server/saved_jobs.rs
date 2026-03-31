use super::ServerState;
use crate::data::{CollectSource, KnownHost, SavedJob, Workflow, load_saved_jobs, save_saved_jobs};
use crate::processor::Identifiers;
use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use datastar::{axum::ReadSignals, consts::ElementPatchMode, patch_elements::PatchElements};
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tokio::{sync::Mutex, task};

#[derive(Template)]
#[template(path = "components/saved_jobs_list.html")]
struct SavedJobsList {
    jobs: Vec<SavedJobListItem>,
}

#[derive(Clone)]
struct SavedJobListItem {
    name: String,
    encoded_name: String,
    is_current: bool,
}

#[derive(Default, Deserialize)]
pub(crate) struct ListSavedJobsSignals {
    #[serde(default)]
    loaded_job_name: String,
}

fn render_saved_jobs_list(jobs: &[String], current_job_name: Option<&str>) -> String {
    let template = SavedJobsList {
        jobs: jobs
            .iter()
            .map(|name| SavedJobListItem {
                name: name.clone(),
                encoded_name: urlencoding::encode(name).into_owned(),
                is_current: current_job_name == Some(name.as_str()),
            })
            .collect(),
    };
    match template.render() {
        Ok(html) => html,
        Err(err) => {
            tracing::error!("Failed to render saved jobs list: {err}");
            String::new()
        }
    }
}

fn patch_saved_jobs_list(jobs: &[String], current_job_name: Option<&str>) -> String {
    let html = render_saved_jobs_list(jobs, current_job_name);
    PatchElements::new(html)
        .selector("#saved-jobs-list")
        .mode(ElementPatchMode::Inner)
        .as_datastar_event()
        .to_string()
}

fn sse_response(events: Vec<String>) -> Response {
    ([(CONTENT_TYPE, "text/event-stream")], events.join("\n\n")).into_response()
}

fn saved_jobs_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn validate_saved_job_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("Job name cannot be empty");
    }

    if name
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | '?' | '#' | '%'))
    {
        return Err("Job name contains unsupported path characters");
    }

    Ok(())
}

pub async fn list_saved_jobs(signals: Option<ReadSignals<ListSavedJobsSignals>>) -> Response {
    let jobs = match load_saved_jobs() {
        Ok(jobs) => jobs,
        Err(err) => {
            tracing::error!("Failed to load saved jobs: {err}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load saved jobs",
            )
                .into_response();
        }
    };
    let names: Vec<String> = jobs.keys().cloned().collect();
    let current_job_name = signals
        .as_ref()
        .map(|ReadSignals(signals)| signals.loaded_job_name.trim())
        .filter(|name| !name.is_empty());
    sse_response(vec![patch_saved_jobs_list(&names, current_job_name)])
}

#[derive(Deserialize)]
pub struct SaveJobSignals {
    pub job_name: String,
    pub metadata: Identifiers,
    pub workflow: Workflow,
}

pub async fn save_job(signals: ReadSignals<SaveJobSignals>) -> Response {
    let ReadSignals(signals) = signals;
    let name = signals.job_name.trim().to_string();
    if let Err(err) = validate_saved_job_name(&name) {
        return (StatusCode::BAD_REQUEST, err).into_response();
    }
    if let Err(err) = validate_saved_job(&signals) {
        return (StatusCode::BAD_REQUEST, err).into_response();
    }

    let saved_job = SavedJob {
        identifiers: signals.metadata,
        workflow: signals.workflow,
    };

    let _guard = saved_jobs_write_lock().lock().await;
    let name_for_save = name.clone();
    let names = match task::spawn_blocking(move || {
        let mut jobs = load_saved_jobs()?;
        jobs.insert(name_for_save, saved_job);
        save_saved_jobs(&jobs)?;
        Ok::<Vec<String>, eyre::Report>(jobs.keys().cloned().collect())
    })
    .await
    {
        Ok(Ok(names)) => names,
        Ok(Err(err)) => {
            tracing::error!("Failed to save jobs: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save jobs").into_response();
        }
        Err(err) => {
            tracing::error!("Saved jobs save task failed: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save jobs").into_response();
        }
    };

    sse_response(vec![patch_saved_jobs_list(&names, Some(name.as_str()))])
}

fn validate_saved_job(signals: &SaveJobSignals) -> Result<(), &'static str> {
    if signals.workflow.collect.mode != crate::data::CollectMode::Collect {
        return Err("Saved jobs require collect mode.");
    }

    if signals.workflow.collect.source != CollectSource::KnownHost {
        return Err("Saved jobs require a known-host collection source.");
    }

    let host_name = signals.workflow.collect.known_host.trim();
    if host_name.is_empty() {
        return Err("Saved jobs require a selected known host.");
    }

    let hosts = KnownHost::parse_hosts_yml().map_err(|_| "Failed to read known hosts.")?;
    hosts
        .get(host_name)
        .ok_or("Saved jobs require a known host that exists in hosts.yml.")?;

    Ok(())
}

pub async fn load_saved_job(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(err) = validate_saved_job_name(&name) {
        return (StatusCode::BAD_REQUEST, err).into_response();
    }
    super::index::jobs_page_with_saved_job(state, name, headers).await
}

pub async fn delete_saved_job(
    Path(name): Path<String>,
    signals: Option<ReadSignals<ListSavedJobsSignals>>,
) -> Response {
    if let Err(err) = validate_saved_job_name(&name) {
        return (StatusCode::BAD_REQUEST, err).into_response();
    }

    let _guard = saved_jobs_write_lock().lock().await;
    let name_for_delete = name.clone();
    let names = match task::spawn_blocking(move || {
        let mut jobs = load_saved_jobs()?;
        if jobs.shift_remove(&name_for_delete).is_none() {
            return Ok::<Option<Vec<String>>, eyre::Report>(None);
        }
        save_saved_jobs(&jobs)?;
        Ok(Some(jobs.keys().cloned().collect()))
    })
    .await
    {
        Ok(Ok(Some(names))) => names,
        Ok(Ok(None)) => return (StatusCode::NOT_FOUND, "Job not found").into_response(),
        Ok(Err(err)) => {
            tracing::error!("Failed to save jobs: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save jobs").into_response();
        }
        Err(err) => {
            tracing::error!("Saved jobs delete task failed: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save jobs").into_response();
        }
    };

    let current_job_name = signals
        .as_ref()
        .map(|ReadSignals(signals)| signals.loaded_job_name.trim())
        .filter(|current| !current.is_empty() && *current != name);
    sse_response(vec![patch_saved_jobs_list(&names, current_job_name)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HostRole, Product};
    use std::{collections::BTreeMap, sync::Mutex};
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        let keystore = tmp.path().join("secrets.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore);
        }
        tmp
    }

    fn save_signals(collect_source: CollectSource, known_host: &str) -> SaveJobSignals {
        let mut workflow = Workflow::default();
        workflow.collect.source = collect_source;
        workflow.collect.known_host = known_host.to_string();

        SaveJobSignals {
            job_name: "test-job".to_string(),
            metadata: Identifiers::default(),
            workflow,
        }
    }

    #[test]
    fn validate_saved_job_allows_known_host_without_secret_reference() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "elasticsearch-local".to_string(),
            KnownHost::NoAuth {
                accept_invalid_certs: false,
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect],
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let result = validate_saved_job(&save_signals(
            CollectSource::KnownHost,
            "elasticsearch-local",
        ));

        assert!(result.is_ok(), "no-auth known hosts should be savable");
    }

    #[test]
    fn validate_saved_job_rejects_non_known_host_sources() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let result = validate_saved_job(&save_signals(CollectSource::ApiKey, ""));

        assert_eq!(
            result.expect_err("api-key jobs should be rejected"),
            "Saved jobs require a known-host collection source."
        );
    }

    #[test]
    fn validate_saved_job_rejects_non_collect_mode() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut signals = save_signals(CollectSource::KnownHost, "elasticsearch-local");
        signals.workflow.collect.mode = crate::data::CollectMode::Upload;

        assert_eq!(
            validate_saved_job(&signals).expect_err("upload mode should be rejected"),
            "Saved jobs require collect mode."
        );
    }

    #[test]
    fn validate_saved_job_name_rejects_path_unsafe_characters() {
        assert_eq!(
            validate_saved_job_name("bad/job").expect_err("slash should be rejected"),
            "Job name contains unsupported path characters"
        );
        assert_eq!(
            validate_saved_job_name("bad%job").expect_err("percent should be rejected"),
            "Job name contains unsupported path characters"
        );
    }

    #[test]
    fn validate_saved_job_name_allows_spaces() {
        assert!(validate_saved_job_name("daily prod collect").is_ok());
    }
}
