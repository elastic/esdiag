// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The universal `Job` model (ADR-0002/0003/0004): one diagnostic execution,
//! composed from six stages within three phases.
//!
//! - **Phase 1 — input (exactly one):** [`Input::Collect`] (new) or
//!   [`Input::Load`] (existing).
//! - **Phase 2 — middle (optional):** [`SaveTarget`] (raw bundle, new only)
//!   and/or [`Process`] (transform). `Export` lives *inside* [`Process`]
//!   (Model β): "process to nowhere" and "export nothing" are unrepresentable.
//! - **Phase 3 — output (optional, and/or):** `Export` (inside `Process`)
//!   and/or [`SendTarget`] (bundle to the Elastic Upload Service).
//!
//! Execution mode is **derived, not stored** (`Save` is the serialization
//! barrier): see [`Job::execution_mode`].

use crate::{
    data::{ElasticContextTarget, Uri},
    processor::{Identifiers, api::ProcessSelection},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Opaque reference to an execution-only resource supplied by
/// [`ExecutionContext`](crate::job::context::ExecutionContext).
///
/// The key is safe to serialize, but the credential, receiver, or adapter it
/// names is deliberately absent from the Job payload.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct BindingKey(String);

impl BindingKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, JobValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(JobValidationError::EmptyRuntimeBinding);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

/// Phase 1: where the diagnostic comes from — exactly one of a *new*
/// collection from live product APIs, or an *existing* bundle.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Input {
    /// Call live product APIs to acquire a new diagnostic.
    Collect {
        host: String,
        diagnostic_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude: Option<Vec<String>>,
    },
    /// Call live product APIs through a symbolic Elastic CLI context reference.
    CollectContext {
        target: ElasticContextTarget,
        diagnostic_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude: Option<Vec<String>>,
    },
    /// Collect through a transient receiver held only by the execution context.
    CollectBinding {
        binding: BindingKey,
        diagnostic_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude: Option<Vec<String>>,
    },
    /// Read an existing diagnostic from a directory, bundle, or download.
    Load { uri: Uri },
    /// Load a transient submitted bundle or a nested path in a parent bundle.
    LoadBinding { binding: BindingKey },
}

impl Input {
    pub fn is_collect(&self) -> bool {
        matches!(
            self,
            Input::Collect { .. } | Input::CollectContext { .. } | Input::CollectBinding { .. }
        )
    }

    pub fn is_sendable_bundle(&self) -> bool {
        matches!(
            self,
            Input::Load {
                uri: Uri::File(_) | Uri::ServiceLink(_)
            } | Input::LoadBinding { .. }
        )
    }

    pub fn uses_runtime_binding(&self) -> bool {
        matches!(self, Input::CollectBinding { .. } | Input::LoadBinding { .. })
    }
}

/// Phase 2a: write freshly collected raw API responses to a bundle.
///
/// `dir: None` materialises the bundle in a temporary directory that is not
/// retained after the job — the bundle still exists during execution (it is
/// the staged-mode serialization barrier and the `Send` source).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SaveTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
}

impl SaveTarget {
    pub fn retained(dir: PathBuf) -> Self {
        Self { dir: Some(dir) }
    }

    pub fn temporary() -> Self {
        Self { dir: None }
    }

    pub fn is_retained(&self) -> bool {
        self.dir.is_some()
    }
}

/// Phase 2b: transform the diagnostic into documents, exporting them to
/// `export`. `Export` ⟺ `Process` is structural: the sink lives here.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Process {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<ProcessSelection>,
    pub export: ExportTarget,
}

/// The destination for *processed* documents (the `Export` stage).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ExportTarget {
    /// A saved known host (an Elasticsearch output cluster).
    KnownHost { name: String },
    /// An Elasticsearch service resolved from Elastic CLI configuration at execution time.
    ElasticContext { target: ElasticContextTarget },
    /// A local newline-delimited JSON file.
    File { path: PathBuf },
    /// A local directory of per-stream files.
    Directory { output_dir: PathBuf },
    /// Standard output.
    Stdout,
    /// A transient document exporter held only by the execution context.
    Binding { binding: BindingKey },
}

/// Phase 3: transmit an existing bundle to a remote destination.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendTarget {
    pub upload_id: String,
}

/// How the executor drives a job that processes (derived from the stage
/// selection, never stored — ADR-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Collection completes and the bundle materialises (the serialization
    /// barrier) before processing reads it. Also covers `Load` input, which
    /// starts from an already-materialised bundle.
    Staged,
    /// Receive, transform, and export overlap concurrently — `Collect` +
    /// `Process` with no `Save`.
    Streaming,
}

/// Construction-time violations of the job invariants. Everything else
/// invalid is unrepresentable in the type.
#[derive(Debug, PartialEq, Eq)]
pub enum JobValidationError {
    /// `save` ⟹ `input` is `Collect`: you save only what you newly collected.
    SaveRequiresCollect,
    /// `send` ⟹ a bundle exists: a `Load` input, or `save` set.
    SendRequiresBundle,
    /// `send` over `Load` currently requires a local bundle archive file.
    SendRequiresArchiveFile,
    /// A job must do something: at least one of `save`/`process`/`send`.
    NoWork,
    /// A temporary save must feed another stage before the temp dir is removed.
    TemporarySaveRequiresConsumer,
    /// Collect jobs need a saved-host reference.
    EmptyCollectHost,
    /// Persisted load inputs must keep their wire semantics across a YAML round trip.
    ResolvedLoadUriNotPersistable,
    /// Runtime binding keys must be non-empty.
    EmptyRuntimeBinding,
    /// Saved jobs must contain only stable, repeatable references.
    RuntimeBindingNotPersistable,
}

impl std::fmt::Display for JobValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SaveRequiresCollect => write!(
                f,
                "`save` requires a `Collect` input: only newly collected diagnostics are saved"
            ),
            Self::SendRequiresBundle => write!(
                f,
                "`send` requires a bundle: a `Load` input, or `save` on a `Collect` input"
            ),
            Self::SendRequiresArchiveFile => write!(
                f,
                "`send` with `Load` requires a local bundle archive file; provide a .zip bundle or collect with `save` enabled"
            ),
            Self::NoWork => write!(f, "a job must select at least one of `save`, `process`, or `send`"),
            Self::TemporarySaveRequiresConsumer => write!(
                f,
                "temporary `save` requires `process` or `send` so the staged bundle is consumed before cleanup"
            ),
            Self::EmptyCollectHost => write!(f, "`Collect` input requires a non-empty host"),
            Self::ResolvedLoadUriNotPersistable => write!(
                f,
                "`Load` input cannot persist resolved host URIs; use a file, directory, URL, or stream reference"
            ),
            Self::EmptyRuntimeBinding => write!(f, "runtime binding key cannot be empty"),
            Self::RuntimeBindingNotPersistable => {
                write!(
                    f,
                    "only stable known-host Collect inputs and stable outputs can be persisted as saved jobs"
                )
            }
        }
    }
}

impl std::error::Error for JobValidationError {}

/// One diagnostic execution: the single model shared by the CLI, the web
/// server, and the executor (ADR-0003/0004).
///
/// Constructed only through [`Job::try_new`], which enforces the phase
/// invariants; the accessors expose the validated stages read-only.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "JobWire")]
pub struct Job {
    #[serde(default, skip_serializing_if = "Identifiers::is_empty")]
    pub identifiers: Identifiers,
    input: Input,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    save: Option<SaveTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process: Option<Process>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    send: Option<SendTarget>,
}

#[derive(Deserialize)]
struct JobWire {
    #[serde(default, skip_serializing_if = "Identifiers::is_empty")]
    identifiers: Identifiers,
    input: Input,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    save: Option<SaveTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    process: Option<Process>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    send: Option<SendTarget>,
}

impl TryFrom<JobWire> for Job {
    type Error = JobValidationError;

    fn try_from(wire: JobWire) -> Result<Self, Self::Error> {
        Job::try_new(wire.identifiers, wire.input, wire.save, wire.process, wire.send)
    }
}

impl Job {
    pub fn try_new(
        identifiers: Identifiers,
        input: Input,
        save: Option<SaveTarget>,
        process: Option<Process>,
        send: Option<SendTarget>,
    ) -> Result<Self, JobValidationError> {
        if save.is_some() && !input.is_collect() {
            return Err(JobValidationError::SaveRequiresCollect);
        }
        if let Input::Collect { host, .. } = &input
            && host.trim().is_empty()
        {
            return Err(JobValidationError::EmptyCollectHost);
        }
        if match &input {
            Input::CollectBinding { binding, .. } | Input::LoadBinding { binding } => binding.is_empty(),
            Input::Collect { .. } | Input::CollectContext { .. } | Input::Load { .. } => false,
        } {
            return Err(JobValidationError::EmptyRuntimeBinding);
        }
        if process
            .as_ref()
            .is_some_and(|process| matches!(&process.export, ExportTarget::Binding { binding } if binding.is_empty()))
        {
            return Err(JobValidationError::EmptyRuntimeBinding);
        }
        if let Input::Load {
            uri: Uri::KnownHost(_) | Uri::ElasticCloud(_) | Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_),
        } = &input
        {
            return Err(JobValidationError::ResolvedLoadUriNotPersistable);
        }
        if send.is_some() && input.is_collect() && save.is_none() {
            return Err(JobValidationError::SendRequiresBundle);
        }
        if send.is_some() && save.is_none() && !input.is_sendable_bundle() {
            return Err(JobValidationError::SendRequiresArchiveFile);
        }
        if input.is_collect() && save.is_none() && process.is_none() && send.is_none() {
            return Err(JobValidationError::NoWork);
        }
        if matches!(&save, Some(SaveTarget { dir: None })) && process.is_none() && send.is_none() {
            return Err(JobValidationError::TemporarySaveRequiresConsumer);
        }
        Ok(Self {
            identifiers,
            input,
            save,
            process,
            send,
        })
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn save(&self) -> Option<&SaveTarget> {
        self.save.as_ref()
    }

    pub fn process(&self) -> Option<&Process> {
        self.process.as_ref()
    }

    pub fn send(&self) -> Option<&SendTarget> {
        self.send.as_ref()
    }

    /// Derive how this job executes: `Save` is the serialization barrier, so
    /// `Collect` + `Process` without `Save` streams; everything else is
    /// staged over a materialised (or loaded) bundle.
    pub fn execution_mode(&self) -> ExecutionMode {
        match (&self.input, &self.save) {
            (Input::Collect { .. } | Input::CollectContext { .. } | Input::CollectBinding { .. }, None)
                if self.process.is_some() =>
            {
                ExecutionMode::Streaming
            }
            _ => ExecutionMode::Staged,
        }
    }

    pub fn uses_runtime_bindings(&self) -> bool {
        self.input.uses_runtime_binding()
            || self
                .process
                .as_ref()
                .is_some_and(|process| matches!(process.export, ExportTarget::Binding { .. }))
    }

    pub fn validate_for_persistence(&self) -> Result<(), JobValidationError> {
        if self.uses_runtime_bindings() || !matches!(self.input, Input::Collect { .. } | Input::CollectContext { .. }) {
            Err(JobValidationError::RuntimeBindingNotPersistable)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Application;

    fn collect_input() -> Input {
        Input::Collect {
            host: "prod".to_string(),
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        }
    }

    fn load_input() -> Input {
        Input::Load {
            uri: Uri::File(PathBuf::from("/tmp/bundle.zip")),
        }
    }

    fn process() -> Process {
        Process {
            selection: None,
            export: ExportTarget::Stdout,
        }
    }

    fn send() -> SendTarget {
        SendTarget {
            upload_id: "abc123".to_string(),
        }
    }

    #[test]
    fn save_requires_collect_input() {
        let err = Job::try_new(
            Identifiers::default(),
            load_input(),
            Some(SaveTarget::temporary()),
            None,
            None,
        )
        .expect_err("save over load must be rejected");
        assert_eq!(err, JobValidationError::SaveRequiresCollect);
    }

    #[test]
    fn send_requires_a_bundle() {
        let err = Job::try_new(Identifiers::default(), collect_input(), None, None, Some(send()))
            .expect_err("collect+send without save must be rejected");
        assert_eq!(err, JobValidationError::SendRequiresBundle);

        // Load input: a local bundle archive satisfies send
        Job::try_new(Identifiers::default(), load_input(), None, None, Some(send())).expect("load+send is valid");

        let err = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::Directory(PathBuf::from("/tmp/bundle")),
            },
            None,
            None,
            Some(send()),
        )
        .expect_err("load+send requires a local archive file");
        assert_eq!(err, JobValidationError::SendRequiresArchiveFile);

        // Collect with save: the materialised bundle satisfies send
        Job::try_new(
            Identifiers::default(),
            collect_input(),
            Some(SaveTarget::temporary()),
            None,
            Some(send()),
        )
        .expect("collect+save+send is valid");
    }

    #[test]
    fn a_job_must_do_something() {
        let err = Job::try_new(Identifiers::default(), collect_input(), None, None, None)
            .expect_err("no-op job must be rejected");
        assert_eq!(err, JobValidationError::NoWork);
    }

    #[test]
    fn load_may_exist_only_for_runtime_retention_policy() {
        let job = Job::try_new(Identifiers::default(), load_input(), None, None, None)
            .expect("Load can be retained by execution policy");

        assert_eq!(
            job.validate_for_persistence(),
            Err(JobValidationError::RuntimeBindingNotPersistable)
        );
    }

    #[test]
    fn collect_input_requires_non_empty_host() {
        let err = Job::try_new(
            Identifiers::default(),
            Input::Collect {
                host: "  ".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            Some(SaveTarget::retained("/tmp/esdiag".into())),
            None,
            None,
        )
        .expect_err("empty collect host must be rejected");
        assert_eq!(err, JobValidationError::EmptyCollectHost);
    }

    #[test]
    fn load_input_rejects_resolved_host_uri() {
        let host = crate::data::KnownHostBuilder::new(url::Url::parse("https://prod.example.com:9200").expect("url"))
            .build()
            .expect("known host");
        let err = Job::try_new(
            Identifiers::default(),
            Input::Load {
                uri: Uri::KnownHost(host),
            },
            None,
            Some(process()),
            None,
        )
        .expect_err("resolved host Uri must not be persisted in jobs.yml");

        assert_eq!(err, JobValidationError::ResolvedLoadUriNotPersistable);
    }

    #[test]
    fn temporary_save_must_feed_another_stage() {
        let err = Job::try_new(
            Identifiers::default(),
            collect_input(),
            Some(SaveTarget { dir: None }),
            None,
            None,
        )
        .expect_err("temporary save without process or send must be rejected");
        assert_eq!(err, JobValidationError::TemporarySaveRequiresConsumer);
    }

    #[test]
    fn deserialization_rejects_invalid_job_shape() {
        let err = yaml_serde::from_str::<Job>(
            r#"
input:
  type: collect
  host: prod
  diagnostic_type: standard
send:
  upload_id: abc123
"#,
        )
        .expect_err("collect+send without save must be rejected");

        assert!(err.to_string().contains("`send` requires a bundle"));
    }

    #[test]
    fn plain_collect_and_save_needs_no_phase_three() {
        let job = Job::try_new(
            Identifiers::default(),
            collect_input(),
            Some(SaveTarget::retained(PathBuf::from("/tmp/out"))),
            None,
            None,
        )
        .expect("collect+save is a valid job");
        assert_eq!(job.execution_mode(), ExecutionMode::Staged);
    }

    #[test]
    fn save_then_process_is_staged() {
        let job = Job::try_new(
            Identifiers::default(),
            collect_input(),
            Some(SaveTarget::temporary()),
            Some(process()),
            None,
        )
        .expect("staged job");
        assert_eq!(job.execution_mode(), ExecutionMode::Staged);
    }

    #[test]
    fn collect_process_without_save_is_streaming() {
        let job =
            Job::try_new(Identifiers::default(), collect_input(), None, Some(process()), None).expect("streaming job");
        assert_eq!(job.execution_mode(), ExecutionMode::Streaming);
    }

    #[test]
    fn load_process_is_staged() {
        let job =
            Job::try_new(Identifiers::default(), load_input(), None, Some(process()), None).expect("load+process job");
        assert_eq!(job.execution_mode(), ExecutionMode::Staged);
    }

    #[test]
    fn full_pipeline_composes_export_and_send() {
        // Save + Process (with Export inside) + Send in one run
        let job = Job::try_new(
            Identifiers::default(),
            collect_input(),
            Some(SaveTarget::retained(PathBuf::from("/tmp/out"))),
            Some(process()),
            Some(send()),
        )
        .expect("save+process+export+send is a valid job");
        assert_eq!(job.execution_mode(), ExecutionMode::Staged);
        assert!(job.process().is_some());
        assert!(job.send().is_some());
    }

    #[test]
    fn ephemeral_runtime_bindings_are_valid_but_not_persistable() {
        let job = Job::try_new(
            Identifiers::default(),
            Input::CollectBinding {
                binding: BindingKey::try_new("request-host").expect("binding key"),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Binding {
                    binding: BindingKey::try_new("service-exporter").expect("binding key"),
                },
            }),
            None,
        )
        .expect("runtime-bound ephemeral job");

        assert!(job.uses_runtime_bindings());
        assert_eq!(
            job.validate_for_persistence(),
            Err(JobValidationError::RuntimeBindingNotPersistable)
        );
    }

    #[test]
    fn deserialization_rejects_empty_runtime_binding_key() {
        let error = yaml_serde::from_str::<Job>(
            r#"
input:
  type: load-binding
  binding: ""
process:
  export:
    type: stdout
"#,
        )
        .expect_err("empty binding must be rejected");

        assert!(error.to_string().contains("runtime binding key cannot be empty"));
    }

    #[test]
    fn service_link_load_can_select_raw_send_for_runtime_materialization() {
        let uri =
            Uri::try_from("https://token:secret@upload.elastic.co/download/file".to_string()).expect("service link");
        assert!(matches!(uri, Uri::ServiceLink(_)));

        Job::try_new(Identifiers::default(), Input::Load { uri }, None, None, Some(send()))
            .expect("service-link bundle can be materialized for send");
    }

    #[test]
    fn elastic_context_references_round_trip_symbolically() {
        let input_target = ElasticContextTarget {
            context: Some("prod".to_string()),
            application: Application::Elasticsearch,
            cloud_deployment: None,
            config_file: None,
        };
        let output_target = ElasticContextTarget {
            context: Some("monitoring".to_string()),
            application: Application::Elasticsearch,
            cloud_deployment: None,
            config_file: None,
        };
        let job = Job::try_new(
            Identifiers::default(),
            Input::CollectContext {
                target: input_target,
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::ElasticContext { target: output_target },
            }),
            None,
        )
        .expect("context job");

        let yaml = yaml_serde::to_string(&job).expect("serialize");
        let loaded: Job = yaml_serde::from_str(&yaml).expect("deserialize");

        assert!(yaml.contains("type: collect-context"));
        assert!(yaml.contains("context: prod"));
        assert!(yaml.contains("type: elastic-context"));
        assert!(yaml.contains("context: monitoring"));
        assert!(!yaml.contains("api_key"));
        assert!(matches!(
            loaded.input(),
            Input::CollectContext { target, .. } if target.context.as_deref() == Some("prod")
        ));
        assert!(matches!(
            loaded.process().map(|process| &process.export),
            Some(ExportTarget::ElasticContext { target })
                if target.context.as_deref() == Some("monitoring")
        ));
    }
}
