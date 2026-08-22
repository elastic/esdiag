use super::ServerState;
use crate::data::{
    CollectSource, Job, JobSignals, KnownHost, is_collectable_app, load_saved_jobs_async, with_saved_jobs_async,
};
use crate::processor::Identifiers;
use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use datastar::{
    axum::ReadSignals, consts::ElementPatchMode, patch_elements::PatchElements, patch_signals::PatchSignals,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

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

#[derive(Default, Deserialize)]
pub struct NormalizeDraftSignals {
    #[serde(default)]
    job: JobSignals,
}

pub async fn normalize_draft(ReadSignals(mut signals): ReadSignals<NormalizeDraftSignals>) -> Response {
    let availability = signals.job.normalize_targets();
    let event = PatchSignals::new(
        json!({
            "job": signals.job,
            "targetAvailability": availability,
        })
        .to_string(),
    )
    .as_datastar_event()
    .to_string();
    sse_response(vec![event])
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

fn validate_saved_job_name(name: &str) -> Result<(), &'static str> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err("Job name cannot be empty");
    }

    if trimmed
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | '?' | '#' | '%'))
    {
        return Err("Job name contains unsupported path characters");
    }

    Ok(())
}

pub async fn list_saved_jobs(signals: Option<ReadSignals<ListSavedJobsSignals>>) -> Response {
    let jobs = match load_saved_jobs_async().await {
        Ok(jobs) => jobs,
        Err(err) => {
            tracing::error!("Failed to load saved jobs: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load saved jobs").into_response();
        }
    };
    let names: Vec<String> = jobs.keys().cloned().collect();
    let current_job_name = signals
        .as_ref()
        .map(|ReadSignals(signals)| signals.loaded_job_name.trim())
        .filter(|name| !name.is_empty());
    sse_response(vec![patch_saved_jobs_list(&names, current_job_name)])
}

#[derive(Clone, Deserialize)]
pub struct SaveJobSignals {
    pub job_name: String,
    pub metadata: Identifiers,
    pub job: JobSignals,
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

    let saved_job = match Job::from_signals(signals.job, signals.metadata) {
        Ok(job) => job,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    };

    let name_for_save = name.clone();
    let names = match with_saved_jobs_async(move |jobs| {
        jobs.insert(name_for_save, saved_job);
        Ok::<(Vec<String>, bool), eyre::Report>((jobs.keys().cloned().collect(), true))
    })
    .await
    {
        Ok(names) => names,
        Err(err) => {
            tracing::error!("Failed to save jobs: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save jobs").into_response();
        }
    };

    sse_response(vec![patch_saved_jobs_list(&names, Some(name.as_str()))])
}

fn validate_saved_job(signals: &SaveJobSignals) -> Result<(), &'static str> {
    if signals.job.collect.mode != crate::data::CollectMode::Collect {
        return Err("Saved jobs require collect mode.");
    }

    if signals.job.collect.source != CollectSource::KnownHost {
        return Err("Saved jobs require a known-host collection source.");
    }

    let host_name = signals.job.collect.known_host.trim();
    if host_name.is_empty() {
        return Err("Saved jobs require a selected known host.");
    }

    let hosts = KnownHost::parse_hosts_yml().map_err(|_| "Failed to read known hosts.")?;
    let host = hosts
        .get(host_name)
        .ok_or("Saved jobs require a known host that exists in hosts.yml.")?;
    if !is_collectable_app(host.app()) {
        return Err("Saved jobs require an Elasticsearch, Kibana, or Logstash collect host.");
    }

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
    let name = name.trim().to_string();
    super::index::jobs_page_with_saved_job(state, name, headers).await
}

pub async fn delete_saved_job(
    Path(name): Path<String>,
    signals: Option<ReadSignals<ListSavedJobsSignals>>,
) -> Response {
    if let Err(err) = validate_saved_job_name(&name) {
        return (StatusCode::BAD_REQUEST, err).into_response();
    }
    let name = name.trim().to_string();

    let name_for_delete = name.clone();
    let names = match with_saved_jobs_async(move |jobs| {
        if jobs.shift_remove(&name_for_delete).is_none() {
            return Ok::<(Option<Vec<String>>, bool), eyre::Report>((None, false));
        }
        Ok((Some(jobs.keys().cloned().collect()), true))
    })
    .await
    {
        Ok(Some(names)) => names,
        Ok(None) => return (StatusCode::NOT_FOUND, "Job not found").into_response(),
        Err(err) => {
            tracing::error!("Failed to delete job: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete job").into_response();
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
    use crate::data::{Application, HostRole, load_saved_jobs};
    use crate::job::model::Input;
    use axum::body::to_bytes;
    use std::collections::BTreeMap;
    use url::Url;

    fn setup_env() -> crate::TestEnv {
        crate::TestEnv::new()
    }

    fn save_signals(collect_source: CollectSource, known_host: &str) -> SaveJobSignals {
        let mut job = JobSignals::default();
        job.collect.source = collect_source;
        job.collect.known_host = known_host.to_string();
        job.collect.save = true;
        job.collect.download_dir = "/tmp/esdiag-saved-job-test".to_string();
        job.process.enabled = false;

        SaveJobSignals {
            job_name: "test-job".to_string(),
            metadata: Identifiers::default(),
            job,
        }
    }

    #[test]
    fn validate_saved_job_allows_known_host_without_secret_reference() {
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "elasticsearch-local".to_string(),
            KnownHost::new_no_auth(
                Application::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let result = validate_saved_job(&save_signals(CollectSource::KnownHost, "elasticsearch-local"));

        assert!(result.is_ok(), "no-auth known hosts should be savable");
    }

    #[test]
    fn validate_saved_job_allows_known_hosts_without_collect_role() {
        let _tmp = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "send-only".to_string(),
            KnownHost::new_no_auth(
                Application::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let result = validate_saved_job(&save_signals(CollectSource::KnownHost, "send-only"));

        assert!(
            result.is_ok(),
            "a saved host should be collectable regardless of its visibility role"
        );
    }

    #[test]
    fn validate_saved_job_rejects_non_known_host_sources() {
        let _tmp = setup_env();

        let result = validate_saved_job(&save_signals(CollectSource::ApiKey, ""));

        assert_eq!(
            result.expect_err("api-key jobs should be rejected"),
            "Saved jobs require a known-host collection source."
        );
    }

    #[test]
    fn validate_saved_job_rejects_non_collect_mode() {
        let _tmp = setup_env();

        let mut signals = save_signals(CollectSource::KnownHost, "elasticsearch-local");
        signals.job.collect.mode = crate::data::CollectMode::Upload;

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

    #[tokio::test]
    async fn save_handler_creates_overwrites_and_rejects_empty_names_without_writing() {
        let _tmp = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "elasticsearch-local".to_string(),
            KnownHost::new_no_auth(
                Application::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let mut signals = save_signals(CollectSource::KnownHost, "elasticsearch-local");
        signals.job_name = "daily".to_string();
        let response = save_job(ReadSignals(signals.clone())).await;
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("save response body");
        let body = String::from_utf8(body.to_vec()).expect("SSE body");
        assert!(
            body.contains("daily"),
            "saved job list must be patched immediately: {body}"
        );

        let jobs_path = std::path::PathBuf::from(std::env::var("HOME").expect("test HOME")).join(".esdiag/jobs.yml");
        let first = std::fs::read_to_string(&jobs_path).expect("saved jobs file");
        assert!(first.contains("schema_version:"));
        assert!(first.contains("daily:"));

        signals.job.collect.diagnostic_type = "support".to_string();
        save_job(ReadSignals(signals)).await;
        let jobs = load_saved_jobs().expect("load overwritten jobs");
        assert_eq!(jobs.len(), 1);
        assert!(matches!(
            jobs["daily"].input(),
            Input::Collect { diagnostic_type, .. } if diagnostic_type == "support"
        ));

        let before_rejection = std::fs::read(&jobs_path).expect("saved jobs bytes");
        let mut empty = save_signals(CollectSource::KnownHost, "elasticsearch-local");
        empty.job_name = "   ".to_string();
        save_job(ReadSignals(empty)).await;
        assert_eq!(
            std::fs::read(&jobs_path).expect("saved jobs after rejection"),
            before_rejection
        );
    }
}
