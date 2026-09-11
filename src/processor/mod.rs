// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Collect diagnostic data from applications
pub mod api;
mod collector;
/// Universal diagnostic processor
pub mod diagnostic;
/// Processors for Elastic Cloud Kubernetes (ECK) diagnostics
mod elastic_cloud_kubernetes;
/// Processors for Elasticsearch diagnostics
mod elasticsearch;
/// Processors for Kibana diagnostics
mod kibana;
/// Processors for Managed Kubernetes Infrastructure (MKI) platform diagnostics
mod kubernetes_platform;
/// Processors for Logstash diagnostics
mod logstash;

pub(crate) use collector::{CollectOptions, collect_bundle};
pub use collector::{CollectionResult, default_collect_archive_name};
pub use diagnostic::{
    DataSource, DiagnosticManifest, DiagnosticReport, Manifest, RequestedApi, SourceContext,
    data_source::init_sources,
    manifest::ManifestBuilder,
    report::{
        BatchResponse, DiagnosticEvent, DiagnosticOutcome, EventSeverity, Identifiers, ProcessorSummary, SkipKind,
    },
};
pub use elasticsearch::Cluster as ElasticsearchCluster;

pub use crate::processor::diagnostic::data_source::StreamingDataSource;
use crate::{
    data::{Application, Platform},
    exporter::{DocumentExporter as StageDocumentExporter, Exporter},
    receiver::Receiver,
};
use api::ProcessSelection;
use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::{ElasticsearchDiagnostic, Licenses};
use eyre::{Result, eyre};
#[cfg(test)]
use futures::FutureExt;
use futures::{
    future::BoxFuture,
    stream::{BoxStream, FuturesUnordered},
};
use kibana::KibanaDiagnostic;
use kubernetes_platform::KubernetesPlatformDiagnostic;
use logstash::LogstashDiagnostic;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::{sync::mpsc, time::Instant};

/// A recognized application whose processor is still being written carries its
/// own message, because "we have not built this yet" and "we deliberately do not
/// do this" are opposite answers to the same question (ADR-0019).
const AGENT_PROCESSING_NOT_IMPLEMENTED: &str = "Elastic Agent processing is not yet implemented";
const UNSUPPORTED_PRODUCT_OR_DIAGNOSTIC_BUNDLE: &str = "Unsupported product or diagnostic bundle";
/// The license holder Elastic Cloud Hosted issues every deployment license to.
const ELASTIC_CLOUD_LICENSE_HOLDER: &str = "Elastic Cloud";
static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

struct Processor<S: State> {
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    child_event_tx: Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
    start_time: Instant,
    pub id: u64,
    pub state: S,
}

/// The `Ready` state represents a new processing job
pub struct Ready {
    manifest: DiagnosticManifest,
    identifiers: Identifiers,
    process_selection: Option<ProcessSelection>,
    #[cfg_attr(not(test), allow(dead_code))]
    process_included_diagnostics: bool,
}

/// The `Processing` state represents an active processing job
pub struct Processing {
    diagnostic: Diagnostic,
    identifiers: Identifiers,
    summary_tx: mpsc::Sender<ProcessorSummary>,
    summary_rx: mpsc::Receiver<ProcessorSummary>,
    report: DiagnosticReport,
    sub_processors: FuturesUnordered<BoxFuture<'static, IncludedDiagnosticOutcome>>,
}

/// The `Completed` state represents a successful processing job
pub struct Completed {
    pub report: DiagnosticReport,
    pub runtime: u128,
    /// Canonical URI for the exporter that received processed documents.
    pub output: String,
    pub included_diagnostics: Vec<IncludedDiagnosticOutcome>,
}

/// The `Failed` state represents a failed processing job
pub struct Failed {
    pub error: String,
    pub runtime: u128,
    pub report: Option<DiagnosticReport>,
    pub included_diagnostics: Vec<IncludedDiagnosticOutcome>,
}

/// The `Status` trait represents the state of a processing job
pub trait State {}
// The Status trait doesn't need any functions, it is only used for trait bounds
impl State for Ready {}
impl State for Processing {}
impl State for Completed {}
impl State for Failed {}

/// The preserved result of one included (child) diagnostic execution. The
/// verdict is the unified [`DiagnosticOutcome`] (ADR-0016) — the same type a
/// parent derives — so a child can be `Partial`, and a skip carries its
/// by-design vs not-implemented kind.
pub struct IncludedDiagnosticOutcome {
    pub job_id: u64,
    pub path: String,
    /// Unified verdict; derived from the child's own report when one exists.
    pub outcome: DiagnosticOutcome,
    /// The child's report, when the child ran far enough to produce one.
    pub report: Option<Box<DiagnosticReport>>,
    pub application: Option<Application>,
    pub platform: Platform,
    /// Skip reason or failure error, when the child did not complete.
    pub reason: Option<String>,
    pub runtime: Option<u128>,
    /// A hard Export-stage failure after this child produced its report.
    /// The report-derived `outcome` remains unchanged.
    pub export_error: Option<String>,
}

pub(crate) struct ProcessStageExecution {
    pub report: DiagnosticReport,
    pub included_diagnostics: Vec<diagnostic::DiagPath>,
    pub export_error: Option<String>,
}

impl IncludedDiagnosticOutcome {
    pub fn job_id(&self) -> u64 {
        self.job_id
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Clone, Debug)]
pub enum IncludedDiagnosticJobEvent {
    Queued {
        job_id: u64,
        path: String,
    },
    Started {
        job_id: u64,
        path: String,
    },
    Completed {
        job_id: u64,
        path: String,
        outcome: DiagnosticOutcome,
        application: Option<Application>,
        platform: Platform,
        diagnostic_id: String,
        docs_created: u32,
        duration_ms: u128,
        kibana_link: Option<String>,
        execution_error: Option<String>,
        recorded_failures: Vec<String>,
    },
    Skipped {
        job_id: u64,
        path: String,
        outcome: DiagnosticOutcome,
        application: Option<Application>,
        platform: Platform,
        reason: String,
    },
    Failed {
        job_id: u64,
        path: String,
        error: String,
    },
}

/// Display label per ADR-0001: the application when present, else the platform.
pub fn display_label(application: Option<Application>, platform: Platform) -> String {
    match application {
        Some(application) => application.to_string(),
        None => platform.to_string(),
    }
}

#[cfg(test)]
fn spawn_sub_processors(
    diag_paths: Vec<diagnostic::DiagPath>,
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    identifiers: Option<Identifiers>,
    child_event_tx: Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
) -> FuturesUnordered<BoxFuture<'static, IncludedDiagnosticOutcome>> {
    let handles = FuturesUnordered::new();
    let identifiers = identifiers.unwrap_or_default();
    for diag_path in diag_paths {
        let child_job_id = new_job_id();
        let path = diag_path.diag_path;
        send_child_event(
            &child_event_tx,
            IncludedDiagnosticJobEvent::Queued {
                job_id: child_job_id,
                path: path.clone(),
            },
        );
        let parent_receiver = receiver.clone();
        let exporter = exporter.clone();
        let ident_clone = identifiers.clone();
        let event_tx = child_event_tx.clone();
        let join_event_tx = child_event_tx.clone();
        let join_path = path.clone();
        let handle = tokio::spawn(async move {
            send_child_event(
                &event_tx,
                IncludedDiagnosticJobEvent::Started {
                    job_id: child_job_id,
                    path: path.clone(),
                },
            );

            let receiver = match parent_receiver.clone_for_subdir(&path) {
                Ok(receiver) => Arc::new(receiver),
                Err(e) => {
                    let outcome = failed_child_outcome(
                        child_job_id,
                        path,
                        format!("Failed to clone receiver for included diagnostic: {e}"),
                    );
                    send_child_outcome_event(&event_tx, &outcome);
                    return outcome;
                }
            };

            let processor = match Processor::try_new_child(receiver, exporter, ident_clone).await {
                Ok(processor) => processor,
                Err(e) => {
                    let outcome = failed_child_outcome(
                        child_job_id,
                        path,
                        format!("Failed to read included diagnostic manifest: {e}"),
                    );
                    send_child_outcome_event(&event_tx, &outcome);
                    return outcome;
                }
            };

            let application = processor.state.manifest.application();
            let platform = processor.state.manifest.platform();
            match processor.start().await {
                Ok(processing) => match processing.process().await {
                    Ok(complete) => {
                        tracing::info!("Included diagnostic processor complete");
                        let report = complete.state.report;
                        // The child's verdict derives from its own report,
                        // exactly as the parent's does (ADR-0016) — a child
                        // can be Partial.
                        let outcome = IncludedDiagnosticOutcome {
                            job_id: child_job_id,
                            path,
                            outcome: report.outcome(),
                            application: report.diagnostic.application,
                            platform: report.diagnostic.platform(),
                            report: Some(Box::new(report)),
                            reason: None,
                            runtime: Some(complete.state.runtime),
                            export_error: None,
                        };
                        send_child_outcome_event(&event_tx, &outcome);
                        outcome
                    }
                    Err(failed) => {
                        let outcome = failed_child_outcome_with_context(
                            child_job_id,
                            path,
                            failed.state.error,
                            application,
                            platform,
                        );
                        send_child_outcome_event(&event_tx, &outcome);
                        outcome
                    }
                },
                Err(failed) => {
                    let outcome = match skip_kind_for(&failed.state.error) {
                        Some(kind) => IncludedDiagnosticOutcome {
                            job_id: child_job_id,
                            path,
                            outcome: DiagnosticOutcome::Skipped(kind),
                            report: None,
                            application,
                            platform,
                            reason: Some(failed.state.error),
                            runtime: None,
                            export_error: None,
                        },
                        None => failed_child_outcome_with_context(
                            child_job_id,
                            path,
                            failed.state.error,
                            application,
                            platform,
                        ),
                    };
                    send_child_outcome_event(&event_tx, &outcome);
                    outcome
                }
            }
        });
        handles.push(
            async move {
                match handle.await {
                    Ok(outcome) => outcome,
                    Err(e) => {
                        tracing::error!("Included diagnostic task panicked or failed to join: {}", e);
                        let outcome = failed_child_outcome(
                            child_job_id,
                            join_path,
                            format!("Included diagnostic task failed to join: {e}"),
                        );
                        send_child_outcome_event(&join_event_tx, &outcome);
                        outcome
                    }
                }
            }
            .boxed(),
        )
    }
    handles
}

#[cfg(test)]
fn failed_child_outcome(job_id: u64, path: String, error: String) -> IncludedDiagnosticOutcome {
    failed_child_outcome_with_context(job_id, path, error, None, Platform::Unknown)
}

#[cfg(test)]
fn failed_child_outcome_with_context(
    job_id: u64,
    path: String,
    error: String,
    application: Option<Application>,
    platform: Platform,
) -> IncludedDiagnosticOutcome {
    IncludedDiagnosticOutcome {
        job_id,
        path,
        outcome: DiagnosticOutcome::Failed,
        report: None,
        application,
        platform,
        reason: Some(error),
        runtime: None,
        export_error: None,
    }
}

/// Why an unsupported child is skipped (ADR-0019): platform-only bundles are
/// out of scope by design; Agent processing is work in progress.
///
/// The by-design boundary governs *collection* — ESDiag will never pull Agent or
/// platform APIs — so it must not be borrowed for an application whose processor
/// is merely unwritten. Reporting one as the other tells a user the feature they
/// are waiting on is never coming.
pub(crate) fn skip_kind_for(error: &str) -> Option<SkipKind> {
    match error {
        AGENT_PROCESSING_NOT_IMPLEMENTED => Some(SkipKind::NotImplemented),
        UNSUPPORTED_PRODUCT_OR_DIAGNOSTIC_BUNDLE => Some(SkipKind::ByDesign),
        _ => None,
    }
}

pub(crate) fn skipped_application(error: &str) -> Option<Application> {
    match error {
        AGENT_PROCESSING_NOT_IMPLEMENTED => Some(Application::Agent),
        _ => None,
    }
}

#[cfg(test)]
fn send_child_event(
    child_event_tx: &Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
    event: IncludedDiagnosticJobEvent,
) {
    if let Some(tx) = child_event_tx {
        let _ = tx.send(event);
    }
}

#[cfg(test)]
fn send_child_outcome_event(
    child_event_tx: &Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
    outcome: &IncludedDiagnosticOutcome,
) {
    let event = match (&outcome.outcome, &outcome.report) {
        (DiagnosticOutcome::Skipped(_), _) => IncludedDiagnosticJobEvent::Skipped {
            job_id: outcome.job_id,
            path: outcome.path.clone(),
            outcome: outcome.outcome,
            application: outcome.application,
            platform: outcome.platform,
            reason: outcome.reason.clone().unwrap_or_default(),
        },
        (_, Some(report)) => IncludedDiagnosticJobEvent::Completed {
            job_id: outcome.job_id,
            path: outcome.path.clone(),
            outcome: outcome.outcome,
            application: report.diagnostic.application,
            platform: report.diagnostic.platform(),
            diagnostic_id: report.diagnostic.metadata.id.clone(),
            docs_created: report.diagnostic.docs.created,
            duration_ms: outcome.runtime.unwrap_or_default(),
            kibana_link: report.diagnostic.kibana_link.clone(),
            execution_error: outcome.export_error.clone(),
            recorded_failures: report
                .events()
                .iter()
                .filter(|event| event.severity != EventSeverity::Success)
                .map(|event| format!("{:?} {}: {}", event.severity, event.source, event.reason))
                .collect(),
        },
        (_, None) => IncludedDiagnosticJobEvent::Failed {
            job_id: outcome.job_id,
            path: outcome.path.clone(),
            error: outcome.reason.clone().unwrap_or_default(),
        },
    };
    send_child_event(child_event_tx, event);
}

/// Resolve the deployment platform for a diagnostic, best-effort (ADR-0001).
///
/// Precedence: an explicit platform on the identifiers (a user override, or a
/// parent diagnostic propagating its platform to a child) wins; then an
/// explicit manifest field (esdiag-written bundles); then a receiver hint
/// (e.g. Elastic Cloud admin API); then manifest indicators; then the
/// `syscalls` folder, which only self-managed collections produce; then the
/// bundle's license holder, which is all an API-only Elastic Cloud Hosted
/// bundle leaves behind. Anything indeterminate is `Unknown` — a first-class,
/// non-failing value.
async fn resolve_platform(manifest: &DiagnosticManifest, receiver: &Receiver, identifiers: &Identifiers) -> Platform {
    if let Some(platform) = identifiers.platform {
        return platform;
    }
    if manifest.has_explicit_platform() {
        return manifest.platform();
    }
    if let Some(platform) = receiver.platform_hint() {
        return platform;
    }
    let detected = manifest.platform();
    if detected != Platform::Unknown {
        return detected;
    }
    if receiver.has_bundle_dir("syscalls").await {
        return Platform::SelfManaged;
    }
    if bundle_license_implies_cloud(receiver).await {
        return Platform::ElasticCloudHosted;
    }
    Platform::Unknown
}

/// Elastic Cloud Hosted issues every deployment's license to a fixed holder,
/// which is often the only platform trace left in an API-only bundle: such
/// bundles carry no `syscalls` folder and no orchestration manifest.
async fn bundle_license_implies_cloud(receiver: &Receiver) -> bool {
    if !receiver.is_bundle() {
        return false;
    }
    matches!(
        receiver.get::<Licenses>().await,
        Ok(licenses) if licenses.license.issued_to() == ELASTIC_CLOUD_LICENSE_HOLDER
    )
}

impl Processor<Ready> {
    async fn try_new_with_options(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
        child_event_tx: Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
        process_included_diagnostics: bool,
    ) -> Result<Self> {
        let mut manifest = receiver.try_get_manifest().await?;
        let platform = resolve_platform(&manifest, &receiver, &identifiers).await;
        manifest.set_platform(platform);
        let identifiers = identifiers.with_platform(platform);
        Ok(Self {
            receiver,
            exporter,
            child_event_tx,
            id: new_job_id(),
            start_time: Instant::now(),
            state: Ready {
                manifest,
                identifiers,
                process_selection,
                process_included_diagnostics,
            },
        })
    }

    #[cfg(test)]
    async fn try_new_child(receiver: Arc<Receiver>, exporter: Arc<Exporter>, identifiers: Identifiers) -> Result<Self> {
        Self::try_new_with_options(receiver, exporter, identifiers, None, None, false).await
    }
}

impl Processor<Ready> {
    /// Executor entry point. Included diagnostics are returned as declarations
    /// so the job executor can run each one with an inherited child context.
    pub(crate) async fn try_new_for_executor(
        receiver: Arc<Receiver>,
        exporter: StageDocumentExporter,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
    ) -> Result<Self> {
        Self::try_new_with_options(
            receiver,
            Arc::new(exporter.into_inner()),
            identifiers,
            process_selection,
            None,
            false,
        )
        .await
    }

    pub(crate) fn included_diagnostics(&self) -> Vec<diagnostic::DiagPath> {
        self.state.manifest.included_diagnostics.clone().unwrap_or_default()
    }

    /// Try creating a processor with the receiver, exporter and identifiers.
    /// Will attempt to build a manifest from a call to the receiver.
    #[cfg(test)]
    async fn try_new(receiver: Arc<Receiver>, exporter: Arc<Exporter>, identifiers: Identifiers) -> Result<Self> {
        Self::try_new_with_selection(receiver, exporter, identifiers, None).await
    }

    #[cfg(test)]
    async fn try_new_with_selection(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
    ) -> Result<Self> {
        Self::try_new_with_options(receiver, exporter, identifiers, process_selection, None, true).await
    }

    /// State transition from `Ready` to `Processing`, returning the progress channel
    async fn start(self) -> Result<Processor<Processing>, Processor<Failed>> {
        tracing::debug!("Transitioned: Processor<Processing>");
        let (summary_tx, summary_rx) = mpsc::channel::<ProcessorSummary>(10);

        // Platform is resolved and set on both the manifest and the
        // identifiers at construction (`try_new_with_options`); children
        // inherit it through the identifiers passed to `spawn_sub_processors`.
        let identifiers = self.state.identifiers.clone();

        if let Some(included_diagnostics) = self.state.manifest.included_diagnostics.clone() {
            let (diagnostic, report) = match Diagnostic::try_new(
                self.receiver.clone(),
                self.exporter.clone(),
                self.state.manifest.clone(),
                self.state.process_selection.clone(),
            )
            .await
            {
                Ok(res) => res,
                Err(err) => {
                    return Err(Processor {
                        receiver: self.receiver,
                        exporter: self.exporter,
                        child_event_tx: self.child_event_tx,
                        start_time: self.start_time,
                        id: self.id,
                        state: Failed {
                            runtime: self.start_time.elapsed().as_millis(),
                            error: err.to_string(),
                            report: None,
                            included_diagnostics: Vec::new(),
                        },
                    });
                }
            };

            #[cfg(test)]
            let mut child_identifiers = identifiers.clone();
            #[cfg(test)]
            if let Some(parent_uuid) = diagnostic.uuid() {
                child_identifiers = child_identifiers.with_parent_id(parent_uuid);
            }

            #[cfg(test)]
            let sub_processors = if self.state.process_included_diagnostics {
                spawn_sub_processors(
                    included_diagnostics,
                    self.receiver.clone(),
                    self.exporter.clone(),
                    Some(child_identifiers),
                    self.child_event_tx.clone(),
                )
            } else {
                FuturesUnordered::new()
            };
            #[cfg(not(test))]
            let sub_processors = {
                let _ = included_diagnostics;
                FuturesUnordered::new()
            };

            let processor = Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                child_event_tx: self.child_event_tx,
                id: self.id,
                start_time: self.start_time,
                state: Processing {
                    diagnostic,
                    identifiers,
                    summary_rx,
                    summary_tx,
                    report,
                    sub_processors,
                },
            };
            return Ok(processor);
        };

        match Diagnostic::try_new(
            self.receiver.clone(),
            self.exporter.clone(),
            self.state.manifest,
            self.state.process_selection,
        )
        .await
        {
            Ok((diagnostic, report)) => {
                let processor = Processor {
                    receiver: self.receiver,
                    exporter: self.exporter,
                    child_event_tx: self.child_event_tx,
                    id: self.id,
                    start_time: self.start_time,
                    state: Processing {
                        diagnostic,
                        identifiers,
                        summary_rx,
                        summary_tx,
                        report,
                        sub_processors: FuturesUnordered::new(),
                    },
                };
                Ok(processor)
            }
            Err(err) => Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                child_event_tx: self.child_event_tx,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: err.to_string(),
                    report: None,
                    included_diagnostics: Vec::new(),
                },
            }),
        }
    }
}

pub(crate) async fn process_documents(
    receiver: Arc<Receiver>,
    exporter: StageDocumentExporter,
    identifiers: Identifiers,
    process_selection: Option<ProcessSelection>,
    discover_included_diagnostics: bool,
) -> Result<ProcessStageExecution> {
    let processor = Processor::try_new_for_executor(receiver, exporter, identifiers, process_selection).await?;
    let included_diagnostics = if discover_included_diagnostics {
        processor.included_diagnostics()
    } else {
        Vec::new()
    };
    let processing = processor.start().await.map_err(|failed| eyre!(failed.state.error))?;
    match processing.process().await {
        Ok(completed) => Ok(ProcessStageExecution {
            report: completed.state.report,
            included_diagnostics,
            export_error: None,
        }),
        Err(mut failed) => match failed.state.report.take() {
            Some(report) => Ok(ProcessStageExecution {
                report,
                included_diagnostics,
                export_error: Some(failed.state.error.clone()),
            }),
            None => Err(eyre!(failed.state.error)),
        },
    }
}

/// The actively `Processing` state.
impl Processor<Processing> {
    #[tracing::instrument(skip_all)]
    async fn process(self) -> Result<Processor<Completed>, Processor<Failed>> {
        tracing::debug!("Processing with async progress updates");

        let Processing {
            diagnostic,
            identifiers,
            summary_tx,
            mut summary_rx,
            report,
            mut sub_processors,
        } = self.state;

        let mut report = report;
        let origin = diagnostic.origin();
        let summary_handle = tokio::spawn(async move {
            while let Some(summary) = summary_rx.recv().await {
                tracing::debug!("{}", summary);
                report.add_processor_summary(summary);
            }
            report
        });

        let process_error = diagnostic.process(summary_tx).await.err();

        // Wait for sub processors to finish
        let mut included_diagnostics = Vec::new();
        while let Some(outcome) = futures::stream::StreamExt::next(&mut sub_processors).await {
            included_diagnostics.push(outcome);
        }

        let mut report = match summary_handle.await {
            Ok(report) => report,
            Err(err) => {
                tracing::error!("Failed to await summary handle: {}", err);
                return Err(Processor {
                    receiver: self.receiver,
                    exporter: self.exporter,
                    child_event_tx: self.child_event_tx,
                    start_time: self.start_time,
                    id: self.id,
                    state: Failed {
                        runtime: self.start_time.elapsed().as_millis(),
                        error: err.to_string(),
                        report: None,
                        included_diagnostics,
                    },
                });
            }
        };

        tracing::info!(
            "Created {} documents for {} diagnostic: {}",
            report.diagnostic.docs.created,
            report.diagnostic.display_label(),
            report.diagnostic.metadata.id,
        );

        if let Some(kibana_link) = self.exporter.kibana_link(
            &report.diagnostic.metadata.id,
            report.diagnostic.metadata.collection_date,
        ) {
            report.add_kibana_link(kibana_link);
        }
        tracing::debug!("{:?}", identifiers);
        report.add_identifiers(identifiers);
        report.add_origin(origin);
        report.add_processing_duration(self.start_time.elapsed().as_millis());
        let output = self.exporter.outcome_uri();
        if let Err(e) = self.exporter.save_report(&report).await {
            tracing::error!("Failed to save report: {}", e);
        }

        if let Some(error) = process_error {
            return Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                child_event_tx: self.child_event_tx,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: error.to_string(),
                    report: Some(report),
                    included_diagnostics,
                },
            });
        }

        Ok(Processor {
            exporter: self.exporter,
            receiver: self.receiver,
            child_event_tx: self.child_event_tx,
            start_time: self.start_time,
            id: self.id,
            state: Completed {
                report,
                runtime: self.start_time.elapsed().as_millis(),
                output,
                included_diagnostics,
            },
        })
    }
}

impl std::fmt::Display for Processor<Failed> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Processor {} failed: {}", self.id, self.state.error)
    }
}

enum Diagnostic {
    Elasticsearch(Box<ElasticsearchDiagnostic>),
    ElasticCloudKubernetes(Box<ElasticCloudKubernetesDiagnostic>),
    KubernetesPlatform(Box<KubernetesPlatformDiagnostic>),
    Kibana(Box<KibanaDiagnostic>),
    Logstash(Box<LogstashDiagnostic>),
}

impl Diagnostic {
    #[cfg(test)]
    pub fn uuid(&self) -> Option<String> {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::ElasticCloudKubernetes(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::KubernetesPlatform(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::Kibana(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::Logstash(diagnostic) => Some(diagnostic.uuid().to_string()),
        }
    }

    pub async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Self, DiagnosticReport)> {
        tracing::info!(
            "Processing {} diagnostic",
            display_label(manifest.application(), manifest.platform())
        );
        tracing::trace!("Diagnostic Manifest: {}", serde_json::to_string(&manifest).unwrap());
        if let Some(selection) = &process_selection
            && manifest.type_key() != selection.product
        {
            return Err(eyre!(
                "Selected processing product '{}' does not match diagnostic product '{}'",
                selection.product,
                manifest.type_key()
            ));
        }
        match manifest.application() {
            Some(Application::Elasticsearch) => {
                let (diagnostic, report) =
                    ElasticsearchDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Elasticsearch(diagnostic), report))
            }
            Some(Application::Logstash) => {
                let (diagnostic, report) =
                    LogstashDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Logstash(diagnostic), report))
            }
            Some(Application::Kibana) => {
                let (diagnostic, report) =
                    KibanaDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Kibana(diagnostic), report))
            }
            Some(Application::Agent) => Err(eyre!(AGENT_PROCESSING_NOT_IMPLEMENTED)),
            // A platform-only diagnostic: dispatch on the platform axis.
            None => match manifest.platform() {
                Platform::ECK => {
                    let (diagnostic, report) =
                        ElasticCloudKubernetesDiagnostic::try_new(receiver, exporter, manifest, process_selection)
                            .await?;
                    Ok((Self::ElasticCloudKubernetes(diagnostic), report))
                }
                Platform::KubernetesPlatform => {
                    let (diagnostic, report) =
                        KubernetesPlatformDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                    Ok((Self::KubernetesPlatform(diagnostic), report))
                }
                _ => Err(eyre!(UNSUPPORTED_PRODUCT_OR_DIAGNOSTIC_BUNDLE)),
            },
        }
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::ElasticCloudKubernetes(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::KubernetesPlatform(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::Kibana(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::Logstash(diagnostic) => diagnostic.process(summary_tx).await,
        }
    }

    fn origin(&self) -> (String, String, String) {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => diagnostic.origin(),
            Diagnostic::ElasticCloudKubernetes(diagnostic) => diagnostic.origin(),
            Diagnostic::KubernetesPlatform(diagnostic) => diagnostic.origin(),
            Diagnostic::Kibana(diagnostic) => diagnostic.origin(),
            Diagnostic::Logstash(diagnostic) => diagnostic.origin(),
        }
    }
}

trait DocumentExporter<T, U> {
    async fn documents_export(self, exporter: &Exporter, lookups: &T, metadata: &U) -> ProcessorSummary;

    #[allow(unused_variables)]
    async fn export_raw(data: String, exporter: &Exporter, lookups: &T, metadata: &U) -> ProcessorSummary {
        ProcessorSummary::new("error".to_string())
    }
}

trait StreamingDocumentExporter<T, U>: StreamingDataSource {
    async fn documents_export_stream(
        stream: BoxStream<'static, Result<Self::Item>>,
        exporter: &Exporter,
        lookups: &T,
        metadata: &U,
    ) -> ProcessorSummary;
}

trait DiagnosticProcessor {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)>;
    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>;
    #[allow(dead_code)]
    fn id(&self) -> &str;
    fn origin(&self) -> (String, String, String);
}

trait Metadata {
    fn as_meta_doc(&self) -> serde_json::Value;
}

pub fn new_job_id() -> u64 {
    NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{KnownHostBuilder, Uri},
        exporter::Exporter,
        receiver::Receiver,
    };
    use serde_json::json;
    use std::collections::HashSet;
    use std::{fs::File, path::Path, sync::Arc};
    use tempfile::TempDir;
    use url::Url;
    use zip::ZipArchive;

    fn archive_path(name: &str) -> String {
        format!("{}/tests/archives/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn extract_archive(name: &str, destination: &Path) {
        std::fs::create_dir_all(destination).expect("create child diagnostic dir");
        let file = File::open(archive_path(name)).expect("open fixture archive");
        let mut archive = ZipArchive::new(file).expect("read fixture archive");
        archive.extract(destination).expect("extract fixture archive");
    }

    fn write_parent_manifest(root: &Path, included_diagnostics: Vec<diagnostic::DiagPath>) {
        let manifest = json!({
            "mode": "support",
            "product": "eck",
            "flags": null,
            "diagnostic": "esdiag-test",
            "type": "eck-diagnostics",
            "runner": "esdiag",
            "version": "3.0.0",
            "timestamp": "2026-01-01T00:00:00Z",
            "collection_date_millis": 1767225600000u64,
            "included_diagnostics": included_diagnostics,
            "identifiers": null,
            "requested_apis": null,
            "collected_apis": null
        });
        std::fs::write(
            root.join(DiagnosticManifest::FILENAME),
            serde_json::to_vec_pretty(&manifest).expect("serialize parent manifest"),
        )
        .expect("write parent manifest");
    }

    async fn process_parent_bundle(root: &Path) -> Processor<Completed> {
        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default())
            .await
            .expect("ready processor");
        let processing = processor
            .start()
            .await
            .map_err(|failed| failed.state.error)
            .expect("processing processor");
        processing
            .process()
            .await
            .map_err(|failed| failed.state.error)
            .expect("completed processor")
    }

    fn parent_with_children(children: &[(&str, &str)]) -> TempDir {
        let root = tempfile::tempdir().expect("parent dir");
        let included = children
            .iter()
            .map(|(path, archive)| {
                extract_archive(archive, &root.path().join(path));
                diagnostic::DiagPath {
                    diag_type: "diagnostic".to_string(),
                    diag_path: (*path).to_string(),
                }
            })
            .collect();
        write_parent_manifest(root.path(), included);
        root
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parent_manifest_returns_multiple_supported_child_outcomes() {
        let root = parent_with_children(&[
            ("child-a", "elasticsearch-api-diagnostics-9.3.3.zip"),
            ("child-b", "elasticsearch-api-diagnostics-8.19.3.zip"),
        ]);

        let completed = process_parent_bundle(root.path()).await;

        assert_eq!(completed.state.included_diagnostics.len(), 2);
        let child_job_ids = completed
            .state
            .included_diagnostics
            .iter()
            .map(IncludedDiagnosticOutcome::job_id)
            .collect::<HashSet<_>>();
        assert_eq!(child_job_ids.len(), 2);
        for child in &completed.state.included_diagnostics {
            // Fixture bundles may omit optional selectable sources; absence in
            // an imported bundle is not a processing failure.
            assert_eq!(child.outcome, DiagnosticOutcome::Complete);
            let report = child.report.as_ref().expect("completed child carries its report");
            assert_eq!(report.diagnostic.application, Some(Application::Elasticsearch));
            assert!(report.diagnostic.docs.created > 0);
            assert!(report.diagnostic.identifiers.parent_id.is_some());
            // Child inherits the parent's platform (ADR-0001)
            assert_eq!(report.diagnostic.identifiers.platform, Some(Platform::ECK));
            assert_eq!(report.diagnostic.platform(), Platform::ECK);
            assert_eq!(report.diagnostic.display_label(), "Elasticsearch");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn kibana_child_returns_completed_outcome() {
        let root = parent_with_children(&[("kibana-child", "kibana-api-diagnostics-9.3.3.zip")]);

        let completed = process_parent_bundle(root.path()).await;

        assert_eq!(completed.state.included_diagnostics.len(), 1);
        let child = &completed.state.included_diagnostics[0];
        assert_eq!(child.outcome, DiagnosticOutcome::Complete);
        assert_eq!(child.platform, Platform::ECK);
        let report = child.report.as_ref().expect("completed child carries its report");
        assert_eq!(report.diagnostic.application, Some(Application::Kibana));
        assert!(report.diagnostic.docs.created > 0);
        assert_eq!(completed.state.report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn kibana_processing() {
        let root = tempfile::tempdir().expect("bundle directory");
        extract_archive("kibana-api-diagnostics-8.19.3.zip", root.path());

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output directory");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default())
            .await
            .expect("ready processor");
        let processing = processor
            .start()
            .await
            .map_err(|failed| failed.state.error)
            .expect("processing processor");
        let completed = processing
            .process()
            .await
            .map_err(|failed| failed.state.error)
            .expect("completed processor");

        assert_eq!(completed.state.report.diagnostic.application, Some(Application::Kibana));
        assert!(completed.state.report.diagnostic.docs.created > 0);
    }

    /// A child diagnostic ESDiag recognizes but has no processor for. Written as
    /// a manifest rather than extracted from a fixture archive because dispatch
    /// happens on the declared application, before any product diagnostic is
    /// constructed.
    fn write_child_manifest(dir: &Path, product: &str) {
        std::fs::create_dir_all(dir).expect("create child diagnostic dir");
        let manifest = json!({
            "mode": "support",
            "product": product,
            "diagnostic": "esdiag-test",
            "type": format!("{product}-diagnostics"),
            "runner": "esdiag",
            "version": "9.3.3",
            "timestamp": "2026-01-01T00:00:00Z",
            "collection_date_millis": 1767225600000u64,
        });
        std::fs::write(
            dir.join(DiagnosticManifest::FILENAME),
            serde_json::to_vec_pretty(&manifest).expect("serialize child manifest"),
        )
        .expect("write child manifest");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn agent_child_is_skipped_as_work_in_progress_not_a_scope_boundary() {
        let root = tempfile::tempdir().expect("parent dir");
        write_child_manifest(&root.path().join("agent-child"), "agent");
        write_parent_manifest(
            root.path(),
            vec![diagnostic::DiagPath {
                diag_type: "diagnostic".to_string(),
                diag_path: "agent-child".to_string(),
            }],
        );

        let completed = process_parent_bundle(root.path()).await;

        let child = &completed.state.included_diagnostics[0];
        // Elastic Agent is out of scope for `Collect` by design, but its
        // *processing* is unwritten work (PR293). Reporting the skip as
        // by-design would say the opposite (ADR-0016/0019).
        assert_eq!(child.outcome, DiagnosticOutcome::Skipped(SkipKind::NotImplemented));
        assert_eq!(child.application, Some(Application::Agent));
        assert!(
            child
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("Elastic Agent processing is not yet implemented")
        );
    }

    #[test]
    fn the_two_gap_kinds_stay_separable() {
        // These cases surface as a skip rather than a processing failure
        // (ADR-0019).
        assert_eq!(
            skip_kind_for(AGENT_PROCESSING_NOT_IMPLEMENTED),
            Some(SkipKind::NotImplemented)
        );
        assert_eq!(
            skip_kind_for(UNSUPPORTED_PRODUCT_OR_DIAGNOSTIC_BUNDLE),
            Some(SkipKind::ByDesign)
        );
        assert_eq!(
            skip_kind_for("Failed to read the child bundle"),
            None,
            "an error that is not a known gap is a failure, not a skip"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unreadable_child_returns_failed_outcome_without_failing_parent() {
        let root = tempfile::tempdir().expect("parent dir");
        write_parent_manifest(
            root.path(),
            vec![diagnostic::DiagPath {
                diag_type: "diagnostic".to_string(),
                diag_path: "missing-child".to_string(),
            }],
        );

        let completed = process_parent_bundle(root.path()).await;

        // A platform-only diagnostic: no application, label falls back to platform
        assert_eq!(completed.state.report.diagnostic.application, None);
        assert_eq!(completed.state.report.diagnostic.platform(), Platform::ECK);
        assert_eq!(completed.state.report.diagnostic.display_label(), "ECK");
        assert_eq!(completed.state.included_diagnostics.len(), 1);
        let child = &completed.state.included_diagnostics[0];
        assert_eq!(child.outcome, DiagnosticOutcome::Failed);
        assert_eq!(child.path, "missing-child");
        assert!(
            child
                .reason
                .as_deref()
                .unwrap_or_default()
                .contains("Failed to read included diagnostic manifest")
        );
    }

    #[test]
    fn failed_child_outcome_preserves_known_child_context() {
        let outcome = failed_child_outcome_with_context(
            42,
            "child-es".to_string(),
            "processing failed".to_string(),
            Some(Application::Elasticsearch),
            Platform::ECK,
        );

        assert_eq!(outcome.outcome, DiagnosticOutcome::Failed);
        assert_eq!(outcome.application, Some(Application::Elasticsearch));
        assert_eq!(outcome.platform, Platform::ECK);
        assert_eq!(outcome.reason.as_deref(), Some("processing failed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn syscalls_folder_implies_self_managed_platform() {
        let root = tempfile::tempdir().expect("bundle dir");
        extract_archive("elasticsearch-api-diagnostics-9.3.3.zip", root.path());
        std::fs::create_dir_all(root.path().join("syscalls")).expect("create syscalls dir");
        let manifest_path = root.path().join(DiagnosticManifest::FILENAME);
        let manifest_before = std::fs::read_to_string(&manifest_path).expect("read manifest before processing");

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default())
            .await
            .expect("ready processor");

        assert_eq!(processor.state.manifest.platform(), Platform::SelfManaged);
        assert_eq!(processor.state.identifiers.platform, Some(Platform::SelfManaged));
        let manifest_after = std::fs::read_to_string(&manifest_path).expect("read manifest after processing");
        assert_eq!(manifest_after, manifest_before);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bundle_without_indicators_resolves_unknown_platform() {
        let root = tempfile::tempdir().expect("bundle dir");
        extract_archive("elasticsearch-api-diagnostics-9.3.3.zip", root.path());

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default())
            .await
            .expect("ready processor");

        assert_eq!(processor.state.manifest.platform(), Platform::Unknown);
        assert_eq!(processor.state.identifiers.platform, Some(Platform::Unknown));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cloud_admin_receiver_implies_elastic_cloud_hosted_platform() {
        let host = KnownHostBuilder::new(Url::parse("https://admin.found.no").expect("url"))
            .apikey(Some("test-api-key".to_string()))
            .build()
            .expect("host");
        let receiver = Receiver::try_from(host).expect("cloud admin receiver");
        let manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-test".to_string()),
            None,
            None,
            Some("support".to_string()),
            None,
            None,
            None,
            Some("8.19.3".to_string()),
        );

        assert_eq!(
            resolve_platform(&manifest, &receiver, &Identifiers::default()).await,
            Platform::ElasticCloudHosted
        );
    }

    fn write_license(bundle: &Path, issued_to: &str) {
        let license = json!({
            "license": {
                "status": "active",
                "uid": "90db30a7-19e4-42e6-b1fc-c76567ada0e2",
                "type": "enterprise",
                "issue_date_in_millis": 1_677_715_200_000_u64,
                "expiry_date_in_millis": 1_835_481_599_999_u64,
                "max_nodes": serde_json::Value::Null,
                "max_resource_units": 100_000,
                "issued_to": issued_to,
                "issuer": "API",
                "start_date_in_millis": 1_677_628_800_000_i64,
            }
        });
        std::fs::write(bundle.join("licenses.json"), license.to_string()).expect("write licenses.json");
    }

    async fn platform_for_bundle_licensed_to(issued_to: &str) -> Platform {
        let root = tempfile::tempdir().expect("bundle dir");
        extract_archive("elasticsearch-api-diagnostics-9.3.3.zip", root.path());
        write_license(root.path(), issued_to);

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default())
            .await
            .expect("ready processor");

        processor.state.manifest.platform()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cloud_license_holder_implies_elastic_cloud_hosted_platform() {
        assert_eq!(
            platform_for_bundle_licensed_to("Elastic Cloud").await,
            Platform::ElasticCloudHosted
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn self_issued_license_holder_leaves_platform_unknown() {
        assert_eq!(
            platform_for_bundle_licensed_to("acme-production").await,
            Platform::Unknown
        );
    }

    #[test]
    fn job_ids_are_monotonic_and_distinct() {
        let first_job = new_job_id();
        let second_job = new_job_id();

        assert!(second_job > first_job);
        assert_ne!(first_job, second_job);
    }
}
