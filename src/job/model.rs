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
//!   and/or [`SendTarget`] (bundle to the Elastic Uploader).
//!
//! Execution mode is **derived, not stored** (`Save` is the serialization
//! barrier): see [`Job::execution_mode`].

use crate::{
    data::{Application, KnownHost, Uri},
    processor::{Identifiers, api::ProcessSelection},
};
use std::path::PathBuf;

/// Phase 1: where the diagnostic comes from — exactly one of a *new*
/// collection from live product APIs, or an *existing* bundle.
#[derive(Clone, Debug)]
pub enum Input {
    /// Call live product APIs to acquire a new diagnostic.
    Collect {
        host: Box<KnownHost>,
        diagnostic_type: String,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
    },
    /// Read an existing diagnostic from a directory, bundle, or download.
    Load { uri: Uri },
}

impl Input {
    pub fn is_collect(&self) -> bool {
        matches!(self, Input::Collect { .. })
    }
}

/// Phase 2a: write freshly collected raw API responses to a bundle.
///
/// `dir: None` materialises the bundle in a temporary directory that is not
/// retained after the job — the bundle still exists during execution (it is
/// the staged-mode serialization barrier and the `Send` source).
#[derive(Clone, Debug)]
pub struct SaveTarget {
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
#[derive(Clone, Debug)]
pub struct Process {
    pub selection: Option<ProcessSelection>,
    pub export: ExportTarget,
}

/// The destination for *processed* documents (the `Export` stage).
#[derive(Clone, Debug)]
pub enum ExportTarget {
    /// A saved known host (an Elasticsearch output cluster).
    KnownHost { name: String },
    /// A local newline-delimited JSON file.
    File { path: PathBuf },
    /// A local directory of per-stream files.
    Directory { dir: PathBuf },
    /// Standard output.
    Stdout,
}

/// Phase 3: transmit an existing bundle to the Elastic Uploader service.
#[derive(Clone, Debug)]
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
    /// `Collect` is scoped to live APIs for Elasticsearch/Kibana/Logstash.
    CollectOutOfScope { app: Option<Application> },
    /// `save` ⟹ `input` is `Collect`: you save only what you newly collected.
    SaveRequiresCollect,
    /// `send` ⟹ a bundle exists: a `Load` input, or `save` set.
    SendRequiresBundle,
    /// A job must do something: at least one of `save`/`process`/`send`.
    NoWork,
}

impl std::fmt::Display for JobValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CollectOutOfScope {
                app: Some(Application::Agent),
            } => write!(
                f,
                "`Collect` is out of scope by design for Elastic Agent: use `read`/`Load` for the Agent-provided diagnostic bundle"
            ),
            Self::CollectOutOfScope { app: None } => write!(
                f,
                "`Collect` is out of scope by design for platform diagnostics: use `read`/`Load` for the platform-generated bundle"
            ),
            Self::CollectOutOfScope { app: Some(app) } => {
                write!(f, "`Collect` is not supported for application {app}")
            }
            Self::SaveRequiresCollect => write!(
                f,
                "`save` requires a `Collect` input: only newly collected diagnostics are saved"
            ),
            Self::SendRequiresBundle => write!(
                f,
                "`send` requires a bundle: a `Load` input, or `save` on a `Collect` input"
            ),
            Self::NoWork => write!(f, "a job must select at least one of `save`, `process`, or `send`"),
        }
    }
}

impl std::error::Error for JobValidationError {}

/// One diagnostic execution: the single model shared by the CLI, the web
/// server, and the executor (ADR-0003/0004).
///
/// Constructed only through [`Job::try_new`], which enforces the phase
/// invariants; the accessors expose the validated stages read-only.
#[derive(Clone, Debug)]
pub struct Job {
    pub identifiers: Identifiers,
    input: Input,
    save: Option<SaveTarget>,
    process: Option<Process>,
    send: Option<SendTarget>,
}

impl Job {
    pub fn try_new(
        identifiers: Identifiers,
        input: Input,
        save: Option<SaveTarget>,
        process: Option<Process>,
        send: Option<SendTarget>,
    ) -> Result<Self, JobValidationError> {
        if let Input::Collect { host, .. } = &input
            && !matches!(
                host.app(),
                Some(Application::Elasticsearch | Application::Kibana | Application::Logstash)
            )
        {
            return Err(JobValidationError::CollectOutOfScope { app: host.app() });
        }
        if save.is_some() && !input.is_collect() {
            return Err(JobValidationError::SaveRequiresCollect);
        }
        if send.is_some() && input.is_collect() && save.is_none() {
            return Err(JobValidationError::SendRequiresBundle);
        }
        if save.is_none() && process.is_none() && send.is_none() {
            return Err(JobValidationError::NoWork);
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
            (Input::Collect { .. }, None) if self.process.is_some() => ExecutionMode::Streaming,
            _ => ExecutionMode::Staged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Application, KnownHostBuilder};
    use url::Url;

    fn host() -> Box<KnownHost> {
        Box::new(
            KnownHostBuilder::new(Url::parse("http://localhost:9200").expect("url"))
                .build()
                .expect("host"),
        )
    }

    fn collect_input() -> Input {
        Input::Collect {
            host: host(),
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        }
    }

    fn collect_input_for(host: KnownHost) -> Input {
        Input::Collect {
            host: Box::new(host),
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

        // Load input: the loaded bundle satisfies send
        Job::try_new(Identifiers::default(), load_input(), None, None, Some(send())).expect("load+send is valid");

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
    fn collect_refuses_agent_by_design() {
        let agent_host = KnownHostBuilder::new(Url::parse("http://localhost:8220").expect("url"))
            .application(Application::Agent)
            .build()
            .expect("host");

        let err = Job::try_new(
            Identifiers::default(),
            collect_input_for(agent_host),
            Some(SaveTarget::temporary()),
            None,
            None,
        )
        .expect_err("agent collect is out of scope");

        assert_eq!(
            err,
            JobValidationError::CollectOutOfScope {
                app: Some(Application::Agent)
            }
        );
        assert!(err.to_string().contains("out of scope by design"));
    }

    #[test]
    fn collect_refuses_platform_by_design() {
        let platform_host = KnownHostBuilder::new_template("https://example.com/{id}".to_string())
            .build()
            .expect("host");

        let err = Job::try_new(
            Identifiers::default(),
            collect_input_for(platform_host),
            Some(SaveTarget::temporary()),
            None,
            None,
        )
        .expect_err("platform collect is out of scope");

        assert_eq!(err, JobValidationError::CollectOutOfScope { app: None });
        assert!(err.to_string().contains("platform-generated bundle"));
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
}
