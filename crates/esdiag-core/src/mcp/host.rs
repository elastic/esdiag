// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    exporter::Exporter,
    job::{admission::JobAdmissionError, context::ExecutionScope, outcome::ExecutionOutcome},
    processor::DiagnosticOutcome,
};
use futures::future::BoxFuture;
use http::HeaderMap;

/// The process that serves MCP tool calls: it resolves who made a request,
/// admits jobs against its concurrency caps, and supplies its exporter.
pub trait McpHost: Send + Sync + 'static {
    /// The identity for a request with `headers`; `None` when this host
    /// resolves no identity. Stdio calls carry no headers.
    fn identity(&self, headers: &HeaderMap) -> eyre::Result<Option<RequestIdentity>>;

    /// Admits one job for `owner`, or reports the cap it would exceed.
    fn admit(&self, owner: String) -> BoxFuture<'_, Result<AdmissionGuard, JobAdmissionError>>;

    /// The exporter that service-mode jobs write to.
    fn exporter(&self) -> BoxFuture<'_, eyre::Result<Exporter>>;
}

/// The host for `esdiag mcp`: no request identity, no caps, and no host exporter.
pub struct StdioHost;

impl McpHost for StdioHost {
    fn identity(&self, _headers: &HeaderMap) -> eyre::Result<Option<RequestIdentity>> {
        Ok(None)
    }

    fn admit(&self, _owner: String) -> BoxFuture<'_, Result<AdmissionGuard, JobAdmissionError>> {
        Box::pin(async { Ok(AdmissionGuard::unlimited()) })
    }

    fn exporter(&self) -> BoxFuture<'_, eyre::Result<Exporter>> {
        Box::pin(async { Err(eyre::eyre!("esdiag mcp has no host exporter")) })
    }
}

/// The user and account a host resolved for one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestIdentity {
    pub user: String,
    pub account: Option<String>,
}

/// How an admitted job ended, passed to the host when its slot is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionRecord {
    Failed,
    Finished {
        outcome: DiagnosticOutcome,
        docs: u32,
        errors: u32,
    },
}

type Release = Box<dyn FnOnce(AdmissionRecord) -> BoxFuture<'static, ()> + Send>;

/// One admitted job's concurrency slot and execution scope. Dropping the
/// guard cancels the scope and releases the slot once every task tracked by
/// the scope has ended, so a cancelled job frees its slot only after its work
/// has stopped.
pub struct AdmissionGuard {
    scope: ExecutionScope,
    record: AdmissionRecord,
    release: Option<Release>,
}

impl AdmissionGuard {
    pub fn new<F, Fut>(release: F) -> Self
    where
        F: FnOnce(AdmissionRecord) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self {
            scope: ExecutionScope::default(),
            record: AdmissionRecord::Failed,
            release: Some(Box::new(move |record| Box::pin(release(record)))),
        }
    }

    /// A guard that holds no slot.
    pub fn unlimited() -> Self {
        Self {
            scope: ExecutionScope::default(),
            record: AdmissionRecord::Failed,
            release: None,
        }
    }

    pub fn scope(&self) -> ExecutionScope {
        self.scope.clone()
    }

    /// Records how the job ended; a guard dropped before this records a failure.
    pub fn finish(&mut self, record: AdmissionRecord) {
        self.record = record;
    }
}

impl AdmissionRecord {
    pub fn from_execution(outcome: &ExecutionOutcome) -> Self {
        match (outcome.failed(), outcome.report.as_ref()) {
            (false, Some(report)) => Self::Finished {
                outcome: report.outcome(),
                docs: report.diagnostic.docs.created,
                errors: report.diagnostic.docs.errors,
            },
            _ => Self::Failed,
        }
    }

    pub fn from_command(result: &eyre::Result<CommandOutcome>) -> Self {
        let diagnostic = match result {
            Ok(CommandOutcome::Completed(CliOutcome::DiagnosticProcessed { diagnostic })) => Some(diagnostic),
            Ok(CommandOutcome::Completed(CliOutcome::JobCompleted {
                process: Some(process), ..
            })) => Some(&process.diagnostic),
            Ok(_) => None,
            Err(_) => return Self::Failed,
        };
        match diagnostic {
            Some(diagnostic) => Self::Finished {
                outcome: diagnostic.outcome,
                docs: diagnostic.documents,
                errors: diagnostic.documents_failed,
            },
            None => Self::Finished {
                outcome: DiagnosticOutcome::Complete,
                docs: 0,
                errors: 0,
            },
        }
    }
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        self.scope.token().cancel();
        self.scope.tracker().close();
        let Some(release) = self.release.take() else {
            return;
        };
        let tracker = self.scope.tracker().clone();
        let record = self.record;
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    tracker.wait().await;
                    release(record).await;
                });
            }
            Err(_) => tracing::error!("MCP job slot not released: no async runtime is running"),
        }
    }
}
