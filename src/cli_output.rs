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
    },
    OutputContext {
        operation: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        config_file: Option<String>,
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
    pub duration_ms: u128,
    pub source: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kibana_url: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn failures_cannot_serialize_credentials_or_error_chains() {
        let error = std::io::Error::other("api_key=very-secret: nested cause");
        let failure = CliFailure::new(CliFailureCategory::AuthenticationFailed, "authentication failed");
        let rendered = serde_json::to_string(&failure).expect("serialize failure");

        assert!(!rendered.contains("very-secret"));
        assert!(!rendered.contains(&error.to_string()));
    }

    #[test]
    fn direct_and_composite_outcomes_preserve_only_selected_durable_stages() {
        let diagnostic = || DiagnosticResult {
            id: "prod-es@2026-08-10~a1b2".to_string(),
            product: "Elasticsearch".to_string(),
            documents: 42,
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
}
