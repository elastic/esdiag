// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ApiKeyRequest, ServerState, UploadServiceRequest};
use crate::{
    data::{KnownHostBuilder, Uri},
    exporter::DocumentExporter,
    job::{
        context::{ExecutionContext, ExecutionIdentity},
        executor::execute_with_context,
        model::{BindingKey, ExportTarget, Input, Job, Process},
        outcome::ExecutionOutcome,
    },
    processor::{DiagnosticOutcome, Identifiers, new_job_id},
    receiver::Receiver,
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use url::Url;

#[derive(Deserialize)]
pub struct ServiceLinkQueryParams {
    #[serde(default, deserialize_with = "deserialize_empty_as_true")]
    wait_for_completion: bool,
}

async fn execute_synchronous_processing(
    state: &ServerState,
    owner: &str,
    job_id: u64,
    metadata: Identifiers,
    input: Input,
    mut context: ExecutionContext,
) -> eyre::Result<ExecutionOutcome> {
    let output_binding = BindingKey::try_new(format!("sync-api-output-{job_id}"))?;
    let exporter = state.exporter.read().await.clone();
    context.bind_document_exporter(output_binding.clone(), DocumentExporter::try_from(exporter)?);
    context = context.with_identity(ExecutionIdentity::new(job_id, owner));
    let job = Job::try_new(
        metadata,
        input,
        None,
        Some(Process {
            selection: None,
            export: ExportTarget::Binding {
                binding: output_binding,
            },
        }),
        None,
    )?;
    let outcome = execute_with_context(job, context).await;
    if outcome.succeeded() {
        Ok(outcome)
    } else {
        Err(eyre::eyre!(execution_failure(&outcome)))
    }
}

fn execution_failure(outcome: &ExecutionOutcome) -> String {
    outcome
        .stages
        .iter()
        .find_map(|stage| match &stage.status {
            crate::job::outcome::StageStatus::Failed(error) | crate::job::outcome::StageStatus::Blocked(error) => {
                Some(error.clone())
            }
            crate::job::outcome::StageStatus::Succeeded | crate::job::outcome::StageStatus::Skipped(_) => None,
        })
        .unwrap_or_else(|| "Diagnostic execution failed".to_string())
}

#[axum::debug_handler]
pub async fn service_link(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(params): Query<ServiceLinkQueryParams>,
    Json(payload): Json<UploadServiceRequest>,
) -> impl IntoResponse {
    tracing::info!("Received JSON elastic uploader request for: {}", payload.url);

    let job_id = new_job_id();

    // Construct the URL with token authentication
    let uploader_service_url = match Url::parse(&payload.url) {
        Ok(mut url) => {
            // Set username to "token" and password to the actual token
            if url.set_username("token").is_err() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Failed to set token in URL"
                    })),
                );
            }
            if payload.token.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Authorization token cannot be empty"
                    })),
                );
            }
            if url.set_password(Some(&payload.token)).is_err() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "Failed to set token in URL"
                    })),
                );
            }
            url
        }
        Err(e) => {
            tracing::error!("Invalid URL provided: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Invalid URL: {}", e)
                })),
            );
        }
    };

    // Create URI from the URL
    let uri = match Uri::try_from(uploader_service_url.to_string()) {
        Ok(uri) => uri,
        Err(e) => {
            tracing::error!("Failed to create URI: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to create URI: {}", e)
                })),
            );
        }
    };

    if matches!(&uri, Uri::Url(_)) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "URL must be for the Elastic Upload Service"
            })),
        );
    }

    let identity = match state.resolve_identity(&headers) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!("Rejecting service_link request due to auth policy: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
    };

    let filename = payload
        .metadata
        .filename
        .clone()
        .unwrap_or_else(|| "Elastic Upload Service".to_string());
    let mut metadata = payload.metadata;
    let owner = identity.user;
    metadata.user = Some(owner.clone());
    if metadata.account.is_none() {
        metadata.account = identity.account;
    }

    if params.wait_for_completion {
        if let Err(err) = state.record_job_started(&owner).await {
            state.record_job_rejected().await;
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
        tracing::info!("Processing service link synchronously: {}", job_id);
        tracing::debug!("[fsm][api.service_link] queued -> processing(sync): job_id={job_id}");

        let binding = BindingKey::try_new(format!("sync-service-link-{job_id}")).expect("valid binding key");
        let mut context = ExecutionContext::default();
        context.inputs.bind_uri(binding.clone(), uri, None);
        match execute_synchronous_processing(
            &state,
            &owner,
            job_id,
            metadata,
            Input::LoadBinding { binding },
            context,
        )
        .await
        {
            Ok(outcome) => {
                let report = outcome.report.as_ref().expect("successful Process has a report");
                state
                    .record_outcome(&owner, report.outcome(), report.diagnostic.docs.errors)
                    .await;
                let response = diagnostic_result_entries(&outcome);

                tracing::info!(
                    "Service link job completed synchronously: {}",
                    report.diagnostic.metadata.id
                );
                (StatusCode::OK, Json(response))
            }
            Err(error) => {
                tracing::error!("Processing failed: {}", error);
                state.record_failure(&owner).await;
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Processing failed: {error}")
                    })),
                )
            }
        }
    } else {
        // Stash the user-scoped metadata and (filename, URI) into the server state for later use
        tracing::debug!("[fsm][api.service_link] queued(in state): job_id={job_id}");
        state.push_link(job_id, metadata, filename, uri).await;

        // Respond with a JSON success
        (StatusCode::CREATED, Json(json!({"link_id": job_id})))
    }
}

#[derive(Deserialize)]
pub struct ApiKeyQueryParams {
    #[serde(default, deserialize_with = "deserialize_empty_as_true")]
    wait_for_completion: bool,
}

/// Custom deserializer that treats empty string or "true" as true, and "false" or absence as false
fn deserialize_empty_as_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt.as_deref() {
        None => Ok(false),
        Some("") | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => Err(serde::de::Error::custom(format!(
            "Invalid boolean value: '{}'. Expected 'true', 'false', or empty string",
            other
        ))),
    }
}

#[axum::debug_handler]
pub async fn api_key(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(params): Query<ApiKeyQueryParams>,
    Json(payload): Json<ApiKeyRequest>,
) -> impl IntoResponse {
    tracing::info!("Received JSON api key request for: {}", payload.url);

    let job_id = new_job_id();
    tracing::debug!(
        "[fsm][api.api_key] start: job_id={}, wait_for_completion={}",
        job_id,
        params.wait_for_completion
    );

    let identity = match state.resolve_identity(&headers) {
        Ok(identity) => identity,
        Err(err) => {
            tracing::warn!("Rejecting api_key request due to auth policy: {err}");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
    };

    // Build the known host from the URL
    let url = match Url::parse(&payload.url) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Failed to parse URL: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to parse URL: {}", e)
                })),
            );
        }
    };

    // Validate apikey is not empty or whitespace-only
    if payload.apikey.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "API key cannot be empty"
            })),
        );
    }

    let host = match KnownHostBuilder::new(url).apikey(Some(payload.apikey)).build() {
        Ok(host) => host,
        Err(e) => {
            tracing::error!("Failed to build host: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Failed to build host: {}", e)
                })),
            );
        }
    };

    let owner = identity.user.clone();

    // If wait_for_completion is true, process the job synchronously
    if params.wait_for_completion {
        if let Err(err) = state.record_job_started(&owner).await {
            state.record_job_rejected().await;
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": err.to_string()
                })),
            );
        }
        tracing::info!("Processing job: {}", job_id);
        tracing::debug!("[fsm][api.api_key] queued -> processing(sync): job_id={job_id}");

        let receiver = match Receiver::try_from(host) {
            Ok(receiver) => {
                tracing::info!("Created receiver: {}", receiver);
                tracing::debug!("[fsm][api.api_key] receiver created: job_id={job_id}");
                receiver
            }
            Err(e) => {
                tracing::error!("Failed to create receiver: {}", e);
                state.record_failure(&owner).await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Failed to create receiver: {}", e)
                    })),
                );
            }
        };

        let mut identifiers = payload.metadata;
        identifiers.user = Some(owner.clone());
        if identifiers.account.is_none() {
            identifiers.account = identity.account.clone();
        }
        let binding = BindingKey::try_new(format!("sync-api-key-{job_id}")).expect("valid binding key");
        let mut context = ExecutionContext::default();
        context.inputs.bind_receiver(binding.clone(), receiver, None);
        match execute_synchronous_processing(
            &state,
            &owner,
            job_id,
            identifiers,
            Input::CollectBinding {
                binding,
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            context,
        )
        .await
        {
            Ok(outcome) => {
                let report = outcome.report.as_ref().expect("successful Process has a report");
                state
                    .record_outcome(&owner, report.outcome(), report.diagnostic.docs.errors)
                    .await;
                let response = diagnostic_result_entries(&outcome);

                tracing::info!("Job completed successfully: {}", report.diagnostic.metadata.id);
                (StatusCode::OK, Json(response))
            }
            Err(error) => {
                tracing::error!("Processing failed: {}", error);
                state.record_failure(&owner).await;
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": format!("Processing failed: {error}")
                    })),
                )
            }
        }
    } else {
        // Stash the username and (filename, URI) into the server state for later use
        tracing::debug!("[fsm][api.api_key] queued(in state): job_id={job_id}");
        let mut metadata = payload.metadata;
        metadata.user = Some(owner);
        if metadata.account.is_none() {
            metadata.account = identity.account;
        }
        state.push_key(job_id, metadata, host, "standard".to_string()).await;

        // Respond with a JSON success
        (StatusCode::CREATED, Json(json!({"key_id": job_id})))
    }
}

fn diagnostic_result_entries(outcome: &ExecutionOutcome) -> Value {
    let report = outcome
        .report
        .as_ref()
        .expect("Process outcome has a diagnostic report");
    let mut entries = vec![json!({
        "status": status_for_outcome(&report.outcome()),
        "outcome": report.outcome().to_string(),
        "diagnostic_id": report.diagnostic.metadata.id,
        "kibana_link": report.diagnostic.kibana_link.as_deref().unwrap_or(""),
        "took": runtime_millis(report.diagnostic.processing_duration),
        "product": report.diagnostic.display_label(),
        "source": "parent"
    })];

    for child in &outcome.children {
        let entry = match (&child.diagnostic_outcome, child.report()) {
            (DiagnosticOutcome::Skipped(_), _) => json!({
                "status": "info",
                "outcome": child.diagnostic_outcome.to_string(),
                "product": crate::processor::display_label(child.application(), child.platform()),
                "source": "included_diagnostic",
                "path": child.path,
                "reason": child.execution_error().unwrap_or_default()
            }),
            (_, Some(report)) => json!({
                "status": if child.export_error().is_some() {
                    "failed"
                } else {
                    status_for_outcome(&child.diagnostic_outcome)
                },
                "outcome": child.diagnostic_outcome.to_string(),
                "diagnostic_id": report.diagnostic.metadata.id,
                "kibana_link": report.diagnostic.kibana_link.as_deref().unwrap_or(""),
                "took": runtime_millis(child.runtime.unwrap_or_default()),
                "product": report.diagnostic.display_label(),
                "source": "included_diagnostic",
                "path": child.path,
                "error": child.export_error().unwrap_or_default()
            }),
            (_, None) => json!({
                "status": status_for_outcome(&child.diagnostic_outcome),
                "outcome": child.diagnostic_outcome.to_string(),
                "product": crate::processor::display_label(child.application(), child.platform()),
                "source": "included_diagnostic",
                "path": child.path,
                "error": child.execution_error().unwrap_or_default()
            }),
        };
        entries.push(entry);
    }

    Value::Array(entries)
}

fn status_for_outcome(outcome: &DiagnosticOutcome) -> &'static str {
    match outcome {
        DiagnosticOutcome::Failed => "failed",
        DiagnosticOutcome::Skipped(_) => "info",
        DiagnosticOutcome::Complete | DiagnosticOutcome::Partial => "success",
    }
}

fn runtime_millis(runtime: u128) -> u64 {
    runtime.try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{diagnostic_result_entries, runtime_millis, status_for_outcome};
    use crate::{
        data::{Application, Platform},
        job::{
            context::ExecutionIdentity,
            outcome::{ChildExecutionOutcome, ExecutionOutcome, Stage, StageStatus},
        },
        processor::{DiagnosticManifest, DiagnosticOutcome, SkipKind, diagnostic::DiagnosticReportBuilder},
    };

    fn report(
        application: Option<Application>,
        platform: Platform,
        id_type: &str,
    ) -> crate::processor::DiagnosticReport {
        let mut manifest = DiagnosticManifest::new(
            "2024-01-01T00:00:00Z".to_string(),
            Some("esdiag-test".to_string()),
            None,
            None,
            Some("standard".to_string()),
            application,
            Some(id_type.to_string()),
            Some("esdiag".to_string()),
            Some("9.3.3".to_string()),
        );
        manifest.set_platform(platform);
        DiagnosticReportBuilder::try_from(manifest)
            .expect("report builder")
            .receiver("Directory /tmp/diag".to_string())
            .build()
            .expect("report")
    }

    #[test]
    fn synchronous_api_results_include_parent_and_child_outcomes() {
        let mut child_report = report(
            Some(Application::Elasticsearch),
            Platform::Unknown,
            "elasticsearch_diagnostic",
        );
        child_report.add_kibana_link("https://kb.example/app/dashboards#/view/child".to_string());
        let export_failed_report = report(
            Some(Application::Elasticsearch),
            Platform::Unknown,
            "elasticsearch_diagnostic",
        );

        let mut parent_report = report(None, Platform::ECK, "eck-diagnostics");
        parent_report.diagnostic.processing_duration = 1_000;
        let mut outcome = ExecutionOutcome::new(ExecutionIdentity::new(1, "test"));
        outcome.report = Some(parent_report);
        let mut child_execution = ExecutionOutcome::new(ExecutionIdentity::new(11, "test"));
        child_execution.report = Some(child_report);
        let mut skipped_execution = ExecutionOutcome::new(ExecutionIdentity::new(12, "test"));
        skipped_execution.record(
            Stage::Process,
            StageStatus::Failed("Kibana processing is not yet implemented".to_string()),
        );
        let mut failed_execution = ExecutionOutcome::new(ExecutionIdentity::new(13, "test"));
        failed_execution.record(Stage::Load, StageStatus::Failed("manifest missing".to_string()));
        let mut export_failed_execution = ExecutionOutcome::new(ExecutionIdentity::new(14, "test"));
        export_failed_execution.report = Some(export_failed_report);
        export_failed_execution.record(Stage::Process, StageStatus::Succeeded);
        export_failed_execution.record(Stage::Export, StageStatus::Failed("child export failed".to_string()));
        outcome.children = vec![
            ChildExecutionOutcome {
                path: "child-es".to_string(),
                diagnostic_outcome: DiagnosticOutcome::Complete,
                execution: Box::new(child_execution),
                application: Some(Application::Elasticsearch),
                platform: Platform::ECK,
                runtime: Some(500),
            },
            ChildExecutionOutcome {
                path: "child-kibana".to_string(),
                diagnostic_outcome: DiagnosticOutcome::Skipped(SkipKind::NotImplemented),
                execution: Box::new(skipped_execution),
                application: Some(Application::Kibana),
                platform: Platform::ECK,
                runtime: None,
            },
            ChildExecutionOutcome {
                path: "child-missing".to_string(),
                diagnostic_outcome: DiagnosticOutcome::Failed,
                execution: Box::new(failed_execution),
                application: None,
                platform: Platform::Unknown,
                runtime: None,
            },
            ChildExecutionOutcome {
                path: "child-export-failed".to_string(),
                diagnostic_outcome: DiagnosticOutcome::Complete,
                execution: Box::new(export_failed_execution),
                application: Some(Application::Elasticsearch),
                platform: Platform::ECK,
                runtime: Some(250),
            },
        ];
        let entries = diagnostic_result_entries(&outcome);
        let entries = entries.as_array().expect("array response");

        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0]["status"], "success");
        assert_eq!(entries[0]["source"], "parent");
        assert_eq!(entries[0]["took"], 1_000);
        assert_eq!(entries[1]["status"], "success");
        assert_eq!(entries[1]["path"], "child-es");
        assert_eq!(entries[1]["took"], 500);
        assert_eq!(
            entries[1]["kibana_link"],
            "https://kb.example/app/dashboards#/view/child"
        );
        assert_eq!(entries[2]["status"], "info");
        assert_eq!(entries[2]["outcome"], "skipped (not implemented)");
        assert_eq!(entries[2]["reason"], "Kibana processing is not yet implemented");
        assert_eq!(entries[3]["status"], "failed");
        assert_eq!(entries[3]["outcome"], "failed");
        assert_eq!(entries[3]["error"], "manifest missing");
        assert_eq!(entries[4]["status"], "failed");
        assert_eq!(entries[4]["outcome"], "complete");
        assert_eq!(entries[4]["error"], "child export failed");
    }

    #[test]
    fn status_for_outcome_tracks_terminal_outcomes() {
        assert_eq!(status_for_outcome(&DiagnosticOutcome::Complete), "success");
        assert_eq!(status_for_outcome(&DiagnosticOutcome::Partial), "success");
        assert_eq!(status_for_outcome(&DiagnosticOutcome::Failed), "failed");
        assert_eq!(
            status_for_outcome(&DiagnosticOutcome::Skipped(SkipKind::ByDesign)),
            "info"
        );
    }

    #[test]
    fn runtime_millis_saturates_at_u64_max() {
        assert_eq!(runtime_millis(u128::from(u64::MAX) + 1), u64::MAX);
    }
}
