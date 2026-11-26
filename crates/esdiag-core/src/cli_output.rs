// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Stable, allowlisted result values emitted by finite CLI commands.
//!
//! Domain types deliberately do not implement this contract directly.  These
//! projections prevent credentials, processor state, and error chains from
//! becoming accidental CLI API surface.

use clap::ValueEnum;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Yaml,
    Json,
}

/// Writes a blank separator, exactly one terminal value, and a trailing newline.
pub fn write_outcome<W: Write, T: Serialize>(writer: &mut W, format: OutputFormat, value: &T) -> io::Result<()> {
    writer.write_all(b"\n")?;
    match format {
        OutputFormat::Yaml => {
            let yaml = yaml_serde::to_string(value).map_err(io::Error::other)?;
            writer.write_all(yaml.as_bytes())?;
            if !yaml.ends_with('\n') {
                writer.write_all(b"\n")?;
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer(&mut *writer, value).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
        }
    }
    writer.flush()
}

/// Writes an outcome to stdout without changing the machine-readable payload.
pub fn write_terminal_outcome<T: Serialize>(format: OutputFormat, value: &T) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    write_outcome(&mut stdout, format, value)
}

/// The result of a finite command: a typed outcome, or output already written
/// to stdout (processed documents or raw text) that leaves no outcome to render.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // one value per command run; boxing buys nothing
pub enum CommandOutcome {
    Completed(CliOutcome),
    Streamed,
}

#[derive(Debug, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CliOutcome {
    ArchiveCollected {
        path: String,
        files: FileCounts,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u128>,
        #[serde(skip_serializing_if = "Option::is_none")]
        send_destination: Option<String>,
    },
    DiagnosticProcessed {
        diagnostic: DiagnosticResult,
    },
    DiagnosticSent {
        destination: String,
    },
    JobCompleted {
        job: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        save: Option<BundleResult>,
        #[serde(skip_serializing_if = "Option::is_none")]
        process: Option<ProcessResult>,
        #[serde(skip_serializing_if = "Option::is_none")]
        send: Option<SendResult>,
    },
    HostsListed {
        hosts: Vec<HostResult>,
    },
    HostAdded {
        host: HostResult,
    },
    HostUpdated {
        host: HostResult,
    },
    HostRemoved {
        name: String,
        path: String,
    },
    HostAuthenticated {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    KeystoreChanged {
        operation: KeystoreOperation,
        #[serde(skip_serializing_if = "Option::is_none")]
        secret_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    KeystoreStatus {
        exists: bool,
        unlock_active: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at_epoch: Option<i64>,
    },
    JobsListed {
        jobs: Vec<SavedJobResult>,
    },
    JobDeleted {
        name: String,
    },
    InitializationCompleted {
        user: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        job: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        skill_installation: Option<AgentSkillsResult>,
    },
    AgentResponse {
        conversation_id: String,
        message: String,
        kibana_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<AgentUsageResult>,
    },
    AgentSkills {
        detected_targets: Vec<String>,
        selected_targets: Vec<String>,
        version: String,
        digest: String,
        results: Vec<AgentSkillTargetResult>,
        reload_guidance: String,
    },
    SetupCompleted {
        targets: Vec<String>,
        outcome: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        failed_indices: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
    },
    OutputContext {
        operation: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    LocalStack {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        native_service: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        esdiag_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kibana_url: Option<String>,
    },
    ServerReady {
        address: String,
        port: u16,
        runtime_mode: String,
        output: String,
    },
    CompletionInstalled {
        shell: String,
        path: String,
        instruction: String,
    },
}

#[derive(Debug, Serialize)]
pub struct FileCounts {
    pub successful: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct AgentUsageResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct AgentSkillsResult {
    pub selected_targets: Vec<String>,
    pub results: Vec<AgentSkillTargetResult>,
    pub recovery_command: String,
}

/// Context retained when an agent-skill installation partially fails.
#[derive(Clone, Debug, Serialize)]
pub struct AgentSkillsFailureContext {
    pub detected_targets: Vec<String>,
    pub selected_targets: Vec<String>,
    pub version: String,
    pub digest: String,
    pub results: Vec<AgentSkillTargetResult>,
    pub reload_guidance: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentSkillTargetResult {
    pub target: String,
    pub action: String,
    pub destination: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BundleResult {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessResult {
    pub diagnostic: DiagnosticResult,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub included: Vec<IncludedDiagnosticResult>,
}

#[derive(Debug, Serialize)]
pub struct SendResult {
    pub destination: String,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticResult {
    pub id: String,
    pub product: String,
    pub documents: u32,
    pub documents_failed: u32,
    pub outcome: crate::processor::DiagnosticOutcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub indexing_failures: Vec<IndexingFailure>,
    pub duration_ms: u128,
    pub source: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kibana_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IndexingFailure {
    pub index: String,
    pub documents_failed: u32,
    pub category: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IncludedDiagnosticResult {
    Completed {
        source: String,
        diagnostic: DiagnosticResult,
    },
    Skipped {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        product: Option<String>,
        reason: String,
    },
    Failed {
        source: String,
        error: String,
    },
}

#[derive(Debug, Serialize)]
pub struct HostResult {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    pub roles: Vec<String>,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_reference: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SavedJobResult {
    pub name: String,
    pub input: JobInputResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save: Option<JobSaveResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<JobProcessResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send: Option<JobSendResult>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobInputResult {
    Collect { host: String, diagnostic_type: String },
    Load { source: String },
}

#[derive(Debug, Serialize)]
pub struct JobSaveResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobProcessResult {
    pub export: String,
}

#[derive(Debug, Serialize)]
pub struct JobSendResult {
    pub upload_id: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeystoreOperation {
    Created,
    Added,
    Updated,
    Removed,
    Unlocked,
    Locked,
    PasswordChanged,
    Migrated,
}

#[derive(Debug, Serialize)]
pub struct CliFailure {
    #[serde(rename = "result")]
    result: &'static str,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_stage: Option<JobStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_safe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<CompletedStages>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<AgentRecovery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_results: Option<Vec<AgentSkillTargetResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_skills: Option<AgentSkillsFailureContext>,
}

impl CliFailure {
    pub fn new(category: CliFailureCategory, message: impl Into<String>) -> Self {
        Self {
            result: "command_failed",
            category: category.as_str().to_string(),
            message: message.into(),
            r#type: None,
            status: None,
            reason: None,
            resource: None,
            failed_stage: None,
            retry_safe: None,
            completed: None,
            recovery: None,
            target_results: None,
            agent_skills: None,
        }
    }

    pub fn resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    pub fn type_(mut self, error_type: impl Into<String>) -> Self {
        self.r#type = Some(error_type.into());
        self
    }

    pub fn http_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn recovery(mut self, recovery: AgentRecovery) -> Self {
        self.recovery = Some(recovery);
        self
    }

    pub fn target_results(mut self, results: Vec<AgentSkillTargetResult>) -> Self {
        self.target_results = Some(results);
        self
    }

    pub fn agent_skills(mut self, context: AgentSkillsFailureContext) -> Self {
        self.agent_skills = Some(context);
        self
    }
}

#[derive(Debug, Serialize)]
pub struct AgentRecovery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kibana_url: Option<String>,
    pub retry_safe: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliFailureCategory {
    InvalidInput,
    Conflict,
    NotFound,
    AuthenticationFailed,
    CollectionFailed,
    ProcessingFailed,
    SendFailed,
    SetupFailed,
    KeystoreFailed,
    Internal,
}

impl CliFailureCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Conflict => "conflict",
            Self::NotFound => "not_found",
            Self::AuthenticationFailed => "authentication_failed",
            Self::CollectionFailed => "collection_failed",
            Self::ProcessingFailed => "processing_failed",
            Self::SendFailed => "send_failed",
            Self::SetupFailed => "setup_failed",
            Self::KeystoreFailed => "keystore_failed",
            Self::Internal => "internal",
        }
    }
}

/// An error raised with the failure category it reports. Displays as the error
/// it holds.
pub struct CategorizedError {
    category: CliFailureCategory,
    message: String,
    source: Option<eyre::Report>,
}

impl CategorizedError {
    pub fn new(category: CliFailureCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            source: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(CliFailureCategory::NotFound, message)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(CliFailureCategory::InvalidInput, message)
    }
}

impl std::fmt::Debug for CategorizedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CategorizedError")
            .field("category", &self.category)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for CategorizedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CategorizedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source.as_ref())
    }
}

/// Names the failure category of an operation's errors. A more specific
/// category raised inside the operation still wins.
pub trait CategorizeFailure<T> {
    fn categorize(self, category: CliFailureCategory) -> eyre::Result<T>;
}

impl<T> CategorizeFailure<T> for eyre::Result<T> {
    fn categorize(self, category: CliFailureCategory) -> eyre::Result<T> {
        self.map_err(|source| {
            eyre::Report::new(CategorizedError {
                category,
                message: source.to_string(),
                source: Some(source),
            })
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    Collect,
    Save,
    Process,
    Send,
}

#[derive(Debug, Serialize)]
pub struct CompletedStages {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save: Option<BundleResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send: Option<SendResult>,
}

/// The category of `error`, from its typed causes and never from its message,
/// which can hold paths and names the user chose.
pub fn classify_failure(error: &eyre::Report) -> CliFailureCategory {
    use crate::job::{JobExecutionFailure, SavedJobNotFound};
    if error
        .chain()
        .any(|cause| cause.is::<crate::data::SecretAlreadyExists>())
    {
        return CliFailureCategory::Conflict;
    }
    if let Some(response) = http_response_details(error) {
        return match response.status {
            401 | 403 => CliFailureCategory::AuthenticationFailed,
            409 => CliFailureCategory::Conflict,
            404 => CliFailureCategory::NotFound,
            500..=599 => CliFailureCategory::Internal,
            _ => CliFailureCategory::InvalidInput,
        };
    }
    if let Some(failure) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<JobExecutionFailure>())
    {
        return failure.stage.failure_category();
    }
    if error.chain().any(|cause| cause.is::<crate::data::KeystoreLocked>()) {
        return CliFailureCategory::KeystoreFailed;
    }
    if error.chain().any(|cause| cause.is::<SavedJobNotFound>()) {
        return CliFailureCategory::NotFound;
    }
    error
        .chain()
        .filter_map(cause_category)
        .last()
        .unwrap_or(CliFailureCategory::InvalidInput)
}

/// The category that one error of a chain names, if any. The innermost one is
/// the most specific.
fn cause_category(cause: &(dyn std::error::Error + 'static)) -> Option<CliFailureCategory> {
    if let Some(failure) = cause.downcast_ref::<CategorizedError>() {
        return Some(failure.category);
    }
    if cause
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Some(CliFailureCategory::NotFound);
    }
    #[cfg(any(feature = "elasticrc", feature = "setup"))]
    if matches!(
        cause.downcast_ref::<elasticrc::Error>(),
        Some(elasticrc::Error::ConfigNotFound { .. } | elasticrc::Error::MissingContext { .. })
    ) {
        return Some(CliFailureCategory::NotFound);
    }
    None
}

pub fn safe_failure_message(error: &eyre::Report) -> String {
    if let Some(response) = http_response_details(error) {
        return match response.status {
            401 => "The server rejected the request because authentication credentials are required.".to_string(),
            403 => "The server rejected the request because the credentials are not authorized.".to_string(),
            404 => "The requested server resource was not found.".to_string(),
            500..=599 => format!(
                "The server failed to complete the request with HTTP {}.",
                response.status
            ),
            status => format!("The server rejected the request with HTTP {status}."),
        };
    }
    match classify_failure(error) {
        CliFailureCategory::Conflict => "requested resource already exists",
        CliFailureCategory::NotFound => "requested resource was not found",
        CliFailureCategory::AuthenticationFailed => "authentication failed",
        CliFailureCategory::CollectionFailed => "diagnostic collection failed",
        CliFailureCategory::ProcessingFailed => "diagnostic processing failed",
        CliFailureCategory::SendFailed => "diagnostic send failed",
        CliFailureCategory::KeystoreFailed => "keystore operation failed",
        CliFailureCategory::SetupFailed => "setup failed",
        CliFailureCategory::Internal | CliFailureCategory::InvalidInput => "command execution failed",
    }
    .to_string()
}

pub struct HttpResponseDetails {
    pub status: u16,
    pub error_type: Option<String>,
    pub reason: Option<String>,
}

pub fn parse_http_response_details(status: u16, body: &str) -> HttpResponseDetails {
    let response_error = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|body| body.get("error").unwrap_or(&body).clone());
    let error_type = response_error
        .as_ref()
        .and_then(|error| error.get("type"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let reason = response_error
        .as_ref()
        .and_then(|error| error.get("reason"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    HttpResponseDetails {
        status,
        error_type,
        reason,
    }
}

pub fn http_response_details(error: &eyre::Report) -> Option<HttpResponseDetails> {
    use crate::receiver::{
        ElasticCloudAdminRequestError, ElasticsearchRequestError, KibanaRequestError, LogstashRequestError,
    };
    for cause in error.chain() {
        if let Some(kibana_sync::Error::ApiResponse { status, body }) = cause.downcast_ref::<kibana_sync::Error>() {
            return Some(parse_http_response_details(status.as_u16(), body));
        }
        if let Some(error) = cause.downcast_ref::<ElasticsearchRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<KibanaRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<LogstashRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<ElasticCloudAdminRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
    }
    None
}

/// `esdiag setup` reports `setup_failed` unless an HTTP response or skill installation supplies the category.
pub fn command_failure(error: &eyre::Report, setup_command: bool) -> CliFailure {
    #[cfg(feature = "agent")]
    let skill_failure = error
        .downcast_ref::<crate::setup::skills::SkillInstallationFailure>()
        .is_some();
    #[cfg(not(feature = "agent"))]
    let skill_failure = false;
    let keystore_locked = error.chain().any(|cause| cause.is::<crate::data::KeystoreLocked>());
    if setup_command && !skill_failure && !keystore_locked && http_response_details(error).is_none() {
        return CliFailure::new(CliFailureCategory::SetupFailed, "setup failed");
    }
    structured_failure(error)
}

const KEYSTORE_LOCKED_MESSAGE: &str =
    "keystore is locked; run `esdiag keystore unlock` from a terminal or set ESDIAG_KEYSTORE_PASSWORD";

pub fn structured_failure(error: &eyre::Report) -> CliFailure {
    use crate::job::{FailedStage, JobExecutionFailure, SavedJobNotFound};
    #[cfg(feature = "agent")]
    if let Some(conflict) = error.downcast_ref::<crate::operations::process::ProcessAskStdoutConflict>() {
        return CliFailure::new(CliFailureCategory::InvalidInput, conflict.to_string());
    }
    if let Some(missing_job) = error.downcast_ref::<SavedJobNotFound>() {
        return CliFailure::new(CliFailureCategory::NotFound, "saved job was not found")
            .resource(missing_job.name.clone());
    }
    #[cfg(feature = "keystore")]
    if let Some(rejected) = error.downcast_ref::<crate::operations::job::SavedJobStdoutRejected>() {
        return CliFailure::new(CliFailureCategory::InvalidInput, rejected.to_string());
    }
    let keystore_locked = error.chain().any(|cause| cause.is::<crate::data::KeystoreLocked>());
    if keystore_locked && error.downcast_ref::<JobExecutionFailure>().is_none() {
        return CliFailure::new(CliFailureCategory::KeystoreFailed, KEYSTORE_LOCKED_MESSAGE);
    }
    #[cfg(feature = "agent")]
    if let Some(agent_failure) = error.downcast_ref::<crate::agent::builder::AgentFailure>() {
        use crate::agent::builder::AgentFailure;
        let category = match agent_failure {
            AgentFailure::Http { status: 401 | 403 } => CliFailureCategory::AuthenticationFailed,
            AgentFailure::Http { status: 409 } => CliFailureCategory::Conflict,
            AgentFailure::Http { status: 404 } => CliFailureCategory::NotFound,
            AgentFailure::Http { status: 500..=599 } | AgentFailure::Remote | AgentFailure::Protocol { .. } => {
                CliFailureCategory::Internal
            }
            AgentFailure::Http { .. } | AgentFailure::Transport | AgentFailure::Interrupted { .. } => {
                CliFailureCategory::InvalidInput
            }
        };
        let mut failure = CliFailure::new(category, agent_failure.to_string());
        if let AgentFailure::Http { status } = agent_failure {
            failure = failure.http_status(*status);
        }
        if let AgentFailure::Interrupted {
            conversation_id,
            kibana_url,
        } = agent_failure
        {
            failure = failure.recovery(AgentRecovery {
                conversation_id: conversation_id.clone(),
                kibana_url: kibana_url.clone(),
                retry_safe: agent_failure.retry_safe(),
            });
        }
        return failure;
    }
    #[cfg(feature = "agent")]
    if let Some(skill_failure) = error.downcast_ref::<crate::setup::skills::SkillInstallationFailure>() {
        return CliFailure::new(CliFailureCategory::Internal, skill_failure.to_string())
            .target_results(skill_failure.context.results.clone())
            .agent_skills(skill_failure.context.clone());
    }
    let Some(job_failure) = error.downcast_ref::<JobExecutionFailure>() else {
        let mut failure = CliFailure::new(classify_failure(error), safe_failure_message(error));
        if let Some(response) = http_response_details(error) {
            failure = failure.http_status(response.status);
            if let Some(error_type) = response.error_type {
                failure = failure.type_(error_type);
            }
            if let Some(reason) = response.reason {
                failure = failure.reason(reason);
            }
        }
        return failure;
    };
    let category = job_failure.stage.failure_category();
    let failed_stage = match job_failure.stage {
        FailedStage::Collect => JobStage::Collect,
        FailedStage::Process => JobStage::Process,
        FailedStage::Send => JobStage::Send,
    };
    let outcome = &job_failure.outcome;
    let completed = CompletedStages {
        save: outcome
            .bundle_path
            .as_ref()
            .filter(|_| outcome.bundle_retained && outcome.bundle_created)
            .map(|path| BundleResult {
                path: path.display().to_string(),
            }),
        process: outcome
            .execution
            .as_ref()
            .and_then(crate::operations::execution_process_result),
        send: outcome.send_slug.as_ref().map(|slug| SendResult {
            destination: format!("https://upload.elastic.co/g/{slug}"),
        }),
    };
    let mut failure = if keystore_locked {
        CliFailure::new(CliFailureCategory::KeystoreFailed, KEYSTORE_LOCKED_MESSAGE)
    } else {
        CliFailure::new(category, safe_failure_message(error))
    };
    failure.failed_stage = Some(failed_stage);
    failure.retry_safe = Some(false);
    if completed.save.is_some() || completed.process.is_some() || completed.send.is_some() {
        failure.completed = Some(completed);
    }
    failure
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyre::WrapErr;

    #[test]
    fn yaml_and_json_have_stable_discriminators_and_blank_separators() {
        let outcome = CliOutcome::ArchiveCollected {
            path: "/tmp/diagnostic.zip".to_string(),
            files: FileCounts {
                successful: 2,
                total: 3,
            },
            duration_ms: None,
            send_destination: None,
        };

        let mut yaml = Vec::new();
        write_outcome(&mut yaml, OutputFormat::Yaml, &outcome).expect("serialize yaml");
        assert_eq!(
            String::from_utf8(yaml).expect("yaml"),
            "\nresult: archive_collected\npath: /tmp/diagnostic.zip\nfiles:\n  successful: 2\n  total: 3\n"
        );

        let mut json = Vec::new();
        write_outcome(&mut json, OutputFormat::Json, &outcome).expect("serialize json");
        assert_eq!(
            String::from_utf8(json).expect("json"),
            "\n{\"result\":\"archive_collected\",\"path\":\"/tmp/diagnostic.zip\",\"files\":{\"successful\":2,\"total\":3}}\n"
        );
    }

    #[test]
    fn agent_response_is_a_finite_yaml_and_json_outcome() {
        let outcome = CliOutcome::AgentResponse {
            conversation_id: "conv-123".to_string(),
            message: "Cluster is healthy.".to_string(),
            kibana_url: "https://kb.example/s/esdiag/app/agent_builder/conversations/conv-123".to_string(),
            usage: Some(AgentUsageResult {
                input_tokens: Some(42),
                output_tokens: Some(7),
            }),
        };

        for format in [OutputFormat::Yaml, OutputFormat::Json] {
            let mut output = Vec::new();
            write_outcome(&mut output, format, &outcome).expect("serialize agent response");
            let value: serde_json::Value = match format {
                OutputFormat::Yaml => yaml_serde::from_slice(&output).expect("parse YAML"),
                OutputFormat::Json => serde_json::from_slice(&output).expect("parse JSON"),
            };
            assert_eq!(value["result"], "agent_response");
            assert_eq!(value["conversation_id"], "conv-123");
            assert_eq!(value["usage"]["input_tokens"], 42);
        }
    }

    #[test]
    fn direct_and_composite_outcomes_preserve_only_selected_durable_stages() {
        let diagnostic = || DiagnosticResult {
            id: "prod-es@2026-08-10~a1b2".to_string(),
            product: "Elasticsearch".to_string(),
            documents: 42,
            documents_failed: 0,
            outcome: crate::processor::DiagnosticOutcome::Complete,
            indexing_failures: vec![],
            duration_ms: 125,
            source: "primary".to_string(),
            output: "stdio://stdout".to_string(),
            kibana_url: None,
        };
        let outcomes = [
            CliOutcome::DiagnosticProcessed {
                diagnostic: diagnostic(),
            },
            CliOutcome::JobCompleted {
                job: "save-only".to_string(),
                save: Some(BundleResult {
                    path: "/tmp/retained.zip".to_string(),
                }),
                process: None,
                send: None,
            },
            CliOutcome::JobCompleted {
                job: "load-send".to_string(),
                save: None,
                process: None,
                send: Some(SendResult {
                    destination: "https://upload.elastic.co/g/a1b2".to_string(),
                }),
            },
            CliOutcome::JobCompleted {
                job: "all-stages".to_string(),
                save: Some(BundleResult {
                    path: "/tmp/retained.zip".to_string(),
                }),
                process: Some(ProcessResult {
                    diagnostic: diagnostic(),
                    included: vec![
                        IncludedDiagnosticResult::Skipped {
                            source: "unsupported".to_string(),
                            product: Some("Kibana".to_string()),
                            reason: "not implemented".to_string(),
                        },
                        IncludedDiagnosticResult::Failed {
                            source: "broken".to_string(),
                            error: "included diagnostic processing failed".to_string(),
                        },
                    ],
                }),
                send: Some(SendResult {
                    destination: "https://upload.elastic.co/g/a1b2".to_string(),
                }),
            },
        ];

        for outcome in outcomes {
            for format in [OutputFormat::Yaml, OutputFormat::Json] {
                let mut output = Vec::new();
                write_outcome(&mut output, format, &outcome).expect("serialize outcome");
                let value: serde_json::Value = match format {
                    OutputFormat::Yaml => yaml_serde::from_slice(&output).expect("parse YAML"),
                    OutputFormat::Json => serde_json::from_slice(&output).expect("parse JSON"),
                };
                assert_eq!(
                    value["result"],
                    if value["job"].is_null() {
                        "diagnostic_processed"
                    } else {
                        "job_completed"
                    }
                );
            }
        }
    }

    #[test]
    fn locked_keystore_job_failure_keeps_recovery_metadata() {
        use crate::job::{FailedStage, JobExecutionFailure, JobOutcome};
        let error = eyre::Report::new(JobExecutionFailure::new(
            FailedStage::Send,
            JobOutcome {
                bundle_path: Some("/tmp/bundle.zip".into()),
                bundle_retained: true,
                bundle_created: true,
                ..JobOutcome::default()
            },
            eyre::Report::new(crate::data::KeystoreLocked { secret_id: None }),
        ));

        let failure = structured_failure(&error);

        assert_eq!(failure.category, "keystore_failed");
        assert!(matches!(failure.failed_stage, Some(JobStage::Send)));
        assert_eq!(failure.retry_safe, Some(false));
        let completed = failure.completed.expect("completed stages");
        assert_eq!(completed.save.expect("saved bundle").path, "/tmp/bundle.zip");
    }

    #[test]
    fn partial_job_failure_exposes_completed_stages_without_a_success_result() {
        let failure = CliFailure {
            result: "command_failed",
            category: CliFailureCategory::SendFailed.as_str().to_string(),
            message: "diagnostic send failed".to_string(),
            r#type: None,
            status: None,
            reason: None,
            resource: None,
            failed_stage: Some(JobStage::Send),
            retry_safe: Some(false),
            completed: Some(CompletedStages {
                save: Some(BundleResult {
                    path: "/tmp/retained.zip".to_string(),
                }),
                process: Some(ProcessResult {
                    diagnostic: DiagnosticResult {
                        id: "prod-es@2026-08-10~a1b2".to_string(),
                        product: "Elasticsearch".to_string(),
                        documents: 42,
                        documents_failed: 0,
                        outcome: crate::processor::DiagnosticOutcome::Complete,
                        indexing_failures: vec![],
                        duration_ms: 125,
                        source: "primary".to_string(),
                        output: "file:///tmp/report.ndjson".to_string(),
                        kibana_url: None,
                    },
                    included: vec![],
                }),
                send: None,
            }),
            recovery: None,
            target_results: None,
            agent_skills: None,
        };

        let value: serde_json::Value = serde_json::to_value(failure).expect("serialize failure");
        assert_eq!(value["result"], "command_failed");
        assert_eq!(value["failed_stage"], "send");
        assert_eq!(value["completed"]["save"]["path"], "/tmp/retained.zip");
        assert!(value.get("diagnostic").is_none());
    }

    #[test]
    fn setup_errors_report_the_setup_failed_category() {
        let error = eyre::Report::new(CategorizedError::not_found("Context 'missing' was not found"));
        assert_eq!(command_failure(&error, true).category, "setup_failed");
        assert_eq!(command_failure(&error, false).category, "not_found");
    }

    #[cfg(feature = "agent")]
    #[test]
    fn failed_agent_skill_installation_preserves_complete_structured_context() {
        let error = eyre::Report::new(crate::setup::skills::SkillInstallationFailure {
            context: AgentSkillsFailureContext {
                detected_targets: vec!["codex".to_string()],
                selected_targets: vec!["claude".to_string(), "codex".to_string()],
                version: "0.18.0-SNAPSHOT".to_string(),
                digest: "a1b2".to_string(),
                results: vec![
                    AgentSkillTargetResult {
                        target: "claude".to_string(),
                        action: "installed".to_string(),
                        destination: "/tmp/.claude/skills/esdiag".to_string(),
                        error: None,
                    },
                    AgentSkillTargetResult {
                        target: "codex".to_string(),
                        action: "conflict".to_string(),
                        destination: "/tmp/.agents/skills/esdiag".to_string(),
                        error: Some("requires --force".to_string()),
                    },
                ],
                reload_guidance: "Restart or reload running coding agents before using the installed skill."
                    .to_string(),
            },
        });

        let value = serde_json::to_value(structured_failure(&error)).expect("serialize failure");
        let context = &value["agent_skills"];
        assert_eq!(context["detected_targets"], serde_json::json!(["codex"]));
        assert_eq!(context["selected_targets"], serde_json::json!(["claude", "codex"]));
        assert_eq!(context["version"], "0.18.0-SNAPSHOT");
        assert_eq!(context["digest"], "a1b2");
        assert_eq!(context["results"][0]["action"], "installed");
        assert_eq!(context["results"][1]["action"], "conflict");
        assert_eq!(
            context["reload_guidance"],
            "Restart or reload running coding agents before using the installed skill."
        );
        assert_eq!(value["target_results"][0]["action"], "installed");
        assert_eq!(value["target_results"][1]["action"], "conflict");
    }

    #[cfg(feature = "agent")]
    #[test]
    fn interrupted_agent_conversation_exposes_only_safe_recovery() {
        let error = eyre::Report::new(crate::agent::builder::AgentFailure::Interrupted {
            conversation_id: Some("conv-123".to_string()),
            kibana_url: Some("https://kb.example/app/agent_builder/conversations/conv-123".to_string()),
        });

        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["recovery"]["conversation_id"], "conv-123");
        assert_eq!(value["recovery"]["retry_safe"], false);
        assert_eq!(
            value["recovery"]["kibana_url"],
            "https://kb.example/app/agent_builder/conversations/conv-123"
        );
    }

    #[test]
    fn messages_do_not_choose_the_category() {
        for message in [
            "Unable to read /Users/me/auth-backups/diag.zip",
            "Failed to write /data/process/out.ndjson",
            "Host 'collect-not found-send' must include role 'view'",
            "/srv/keystore/credentials/secret.yml is a directory",
        ] {
            let error = eyre::eyre!("{message}");
            assert_eq!(classify_failure(&error), CliFailureCategory::InvalidInput, "{message}");
            assert_eq!(safe_failure_message(&error), "command execution failed", "{message}");
        }
    }

    #[test]
    fn categorized_errors_keep_their_message_and_the_most_specific_category() {
        let error = Err::<(), _>(eyre::eyre!(
            "Keystore path '/srv/collect/secrets.yml' has no parent directory"
        ))
        .categorize(CliFailureCategory::KeystoreFailed)
        .wrap_err("Failed to process /auth/send")
        .unwrap_err();
        assert_eq!(classify_failure(&error), CliFailureCategory::KeystoreFailed);

        let error = Err::<(), _>(eyre::Report::new(CategorizedError::not_found(
            "Secret 'collect' not found",
        )))
        .categorize(CliFailureCategory::KeystoreFailed)
        .unwrap_err();
        assert_eq!(classify_failure(&error), CliFailureCategory::NotFound);
        assert_eq!(error.to_string(), "Secret 'collect' not found");

        let error = Err::<(), _>(eyre::Report::new(crate::receiver::ElasticsearchRequestError::new(
            elasticsearch::http::StatusCode::UNAUTHORIZED,
            String::new(),
            5,
            0,
        )))
        .categorize(CliFailureCategory::SendFailed)
        .unwrap_err();
        assert_eq!(classify_failure(&error), CliFailureCategory::AuthenticationFailed);
        assert_eq!(structured_failure(&error).status, Some(401));
    }

    #[test]
    fn typed_not_found_errors_are_not_found() {
        let missing_file = eyre::Report::new(std::io::Error::from(std::io::ErrorKind::NotFound))
            .wrap_err("Failed to open /data/process/diag.zip");
        assert_eq!(classify_failure(&missing_file), CliFailureCategory::NotFound);

        let unreadable = eyre::Report::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            .wrap_err("Failed to open /data/not found/diag.zip");
        assert_eq!(classify_failure(&unreadable), CliFailureCategory::InvalidInput);

        #[cfg(any(feature = "elasticrc", feature = "setup"))]
        {
            let missing_context = eyre::Report::new(elasticrc::Error::MissingContext {
                name: "diag".to_string(),
                available: vec!["prod".to_string()],
            });
            assert_eq!(classify_failure(&missing_context), CliFailureCategory::NotFound);
            let missing_service = eyre::Report::new(elasticrc::Error::MissingService {
                context: "prod".to_string(),
                service: "kibana",
            });
            assert_eq!(classify_failure(&missing_service), CliFailureCategory::InvalidInput);
        }
    }

    #[test]
    fn job_failures_report_the_failed_stage() {
        use crate::job::{FailedStage, JobExecutionFailure, JobOutcome};
        for (stage, category, message) in [
            (
                FailedStage::Collect,
                "collection_failed",
                "diagnostic collection failed",
            ),
            (
                FailedStage::Process,
                "processing_failed",
                "diagnostic processing failed",
            ),
            (FailedStage::Send, "send_failed", "diagnostic send failed"),
        ] {
            let failure = structured_failure(&eyre::Report::new(JobExecutionFailure::new(
                stage,
                JobOutcome::default(),
                eyre::eyre!("Unable to open /auth/not found/diag.zip"),
            )));
            assert_eq!(failure.category, category);
            assert_eq!(failure.message, message);
        }

        let wrapped = Err::<(), _>(eyre::Report::new(JobExecutionFailure::new(
            FailedStage::Process,
            JobOutcome::default(),
            eyre::eyre!("Unable to send /auth/diag.zip"),
        )))
        .categorize(CliFailureCategory::SendFailed)
        .unwrap_err();
        assert_eq!(classify_failure(&wrapped), CliFailureCategory::ProcessingFailed);
    }

    #[test]
    fn unauthorized_elasticsearch_responses_are_authentication_failures() {
        let error = eyre::Report::new(crate::receiver::ElasticsearchRequestError::new(
            elasticsearch::http::StatusCode::UNAUTHORIZED,
            "missing authentication credentials".to_string(),
            5,
            34,
        ));

        assert_eq!(classify_failure(&error), CliFailureCategory::AuthenticationFailed);
    }

    #[test]
    fn conflict_responses_are_conflict_failures() {
        let error = eyre::Report::new(crate::receiver::KibanaRequestError::new(
            reqwest::StatusCode::CONFLICT,
            r#"{"statusCode":409,"error":"Conflict","message":"resource already exists"}"#.to_string(),
            5,
            80,
        ));

        assert_eq!(classify_failure(&error), CliFailureCategory::Conflict);
    }

    #[test]
    fn kibana_sync_conflicts_are_conflict_failures() {
        let error = eyre::Report::new(kibana_sync::Error::context(
            "workflow sync failed",
            kibana_sync::Error::api_response(
                reqwest::StatusCode::CONFLICT,
                r#"{"error":"Conflict","message":"workflow already exists","statusCode":409}"#,
            ),
        ));

        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["category"], CliFailureCategory::Conflict.as_str());
        assert_eq!(value["status"], 409);
    }

    #[test]
    fn http_failures_include_status_type_and_reason() {
        let error = eyre::Report::new(crate::receiver::ElasticsearchRequestError::new(
            elasticsearch::http::StatusCode::UNAUTHORIZED,
            r#"{"error":{"type":"security_exception","reason":"missing authentication credentials for REST request [api_key=secret]"},"status":401}"#
                .to_string(),
            5,
            133,
        ));
        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["category"], CliFailureCategory::AuthenticationFailed.as_str());
        assert_eq!(value["status"], 401);
        assert_eq!(value["type"], "security_exception");
        assert_eq!(
            value["reason"],
            "missing authentication credentials for REST request [api_key=secret]"
        );
        assert_eq!(
            value["message"],
            "The server rejected the request because authentication credentials are required."
        );
    }

    #[test]
    fn non_elasticsearch_http_failures_use_the_shared_response_projection() {
        use crate::receiver::{ElasticCloudAdminRequestError, KibanaRequestError, LogstashRequestError};

        let body = r#"{"error":{"type":"upstream_failure","reason":"remote dependency was unavailable"}}"#.to_string();
        let failures = [
            (
                eyre::Report::new(KibanaRequestError::new(
                    reqwest::StatusCode::TOO_MANY_REQUESTS,
                    body.clone(),
                    5,
                    80,
                )),
                429,
                CliFailureCategory::InvalidInput,
            ),
            (
                eyre::Report::new(LogstashRequestError::new(
                    reqwest::StatusCode::NOT_FOUND,
                    body.clone(),
                    5,
                    80,
                )),
                404,
                CliFailureCategory::NotFound,
            ),
            (
                eyre::Report::new(ElasticCloudAdminRequestError::new(
                    reqwest::StatusCode::BAD_GATEWAY,
                    body,
                    5,
                    80,
                )),
                502,
                CliFailureCategory::Internal,
            ),
        ];

        for (error, status, category) in failures {
            let value = serde_json::to_value(structured_failure(&error)).expect("serialize failure");
            assert_eq!(value["category"], category.as_str());
            assert_eq!(value["status"], status);
            assert_eq!(value["type"], "upstream_failure");
            assert_eq!(value["reason"], "remote dependency was unavailable");
        }
    }

    #[test]
    fn structured_failure_never_serializes_secret_bearing_error_chains() {
        let error = eyre::Report::new(std::io::Error::other("api_key=very-secret: nested cause"))
            .wrap_err("authorization header ApiKey very-secret-too was rejected");

        let failure = structured_failure(&error);
        let json = serde_json::to_string(&failure).expect("serialize failure");
        let yaml = yaml_serde::to_string(&failure).expect("serialize failure as YAML");

        for rendered in [json, yaml] {
            assert!(!rendered.contains("very-secret"), "secret leaked: {rendered}");
            assert!(!rendered.contains("nested cause"), "error chain leaked: {rendered}");
        }
    }
}
