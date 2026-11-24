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
/// Syscall source catalog and shared syscall processing runner
pub mod syscall;

pub(crate) use collector::{CollectOptions, collect_bundle};
pub use collector::{CollectionResult, default_collect_archive_name};
pub use diagnostic::{
    BundleOrigin, DataSource, DiagnosticManifest, DiagnosticReport, Manifest, RequestedApi, SourceContext,
    data_source::init_sources,
    manifest::ManifestBuilder,
    report::{
        BatchResponse, DiagnosticEvent, DiagnosticOutcome, EventSeverity, Identifiers, ProcessorSummary, SkipKind,
    },
};
pub use elasticsearch::Cluster as ElasticsearchCluster;

pub use crate::processor::diagnostic::data_source::StreamingDataSource;
use crate::{
    data::{self, Application, Platform},
    exporter::{DocumentExporter as StageDocumentExporter, Exporter, KibanaLinkTarget},
    receiver::Receiver,
};
use api::ProcessSelection;
use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::{ElasticsearchDiagnostic, Licenses};
use eyre::{Result, eyre};
use futures::stream::BoxStream;
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
    start_time: Instant,
    pub id: u64,
    pub state: S,
}

/// The `Ready` state represents a new processing job
pub struct Ready {
    manifest: DiagnosticManifest,
    identifiers: Identifiers,
    process_selection: Option<ProcessSelection>,
}

/// The `Processing` state represents an active processing job
pub struct Processing {
    diagnostic: Diagnostic,
    identifiers: Identifiers,
    summary_tx: mpsc::Sender<ProcessorSummary>,
    summary_rx: mpsc::Receiver<ProcessorSummary>,
    report: DiagnosticReport,
}

/// The `Completed` state represents a successful processing job
pub struct Completed {
    pub report: DiagnosticReport,
    pub runtime: u128,
    /// Canonical URI for the exporter that received processed documents.
    pub output: String,
}

/// The `Failed` state represents a failed processing job
pub struct Failed {
    pub error: String,
    pub runtime: u128,
    pub report: Option<DiagnosticReport>,
}

/// The `Status` trait represents the state of a processing job
pub trait State {}
// The Status trait doesn't need any functions, it is only used for trait bounds
impl State for Ready {}
impl State for Processing {}
impl State for Completed {}
impl State for Failed {}

pub(crate) struct ProcessStageExecution {
    pub report: DiagnosticReport,
    pub included_diagnostics: Vec<diagnostic::DiagPath>,
    pub export_error: Option<String>,
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
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
    ) -> Result<Self> {
        let mut manifest = receiver.try_get_manifest().await?;
        let platform = resolve_platform(&manifest, &receiver, &identifiers).await;
        manifest.set_platform(platform);
        let identifiers = identifiers.with_platform(platform);
        Ok(Self {
            receiver,
            exporter,
            id: new_job_id(),
            start_time: Instant::now(),
            state: Ready {
                manifest,
                identifiers,
                process_selection,
            },
        })
    }

    /// Executor entry point. Included diagnostics are returned as declarations
    /// so the job executor can run each one with an inherited child context.
    pub(crate) async fn try_new_for_executor(
        receiver: Arc<Receiver>,
        exporter: StageDocumentExporter,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
    ) -> Result<Self> {
        Self::try_new(
            receiver,
            Arc::new(exporter.into_inner()),
            identifiers,
            process_selection,
        )
        .await
    }

    pub(crate) fn included_diagnostics(&self) -> Vec<diagnostic::DiagPath> {
        self.state
            .manifest
            .child_diagnostics()
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    /// State transition from `Ready` to `Processing`, returning the progress channel
    async fn start(self) -> Result<Processor<Processing>, Processor<Failed>> {
        tracing::debug!("Transitioned: Processor<Processing>");
        let (summary_tx, summary_rx) = mpsc::channel::<ProcessorSummary>(10);

        // Platform is resolved and set on both the manifest and the
        // identifiers at construction (`try_new`).
        let identifiers = self.state.identifiers.clone();

        match Diagnostic::try_new(
            self.receiver.clone(),
            self.exporter.clone(),
            self.state.manifest,
            self.state.process_selection,
        )
        .await
        {
            Ok((diagnostic, report)) => Ok(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                id: self.id,
                start_time: self.start_time,
                state: Processing {
                    diagnostic,
                    identifiers,
                    summary_rx,
                    summary_tx,
                    report,
                },
            }),
            Err(err) => Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: err.to_string(),
                    report: None,
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

        let mut report = match summary_handle.await {
            Ok(report) => report,
            Err(err) => {
                tracing::error!("Failed to await summary handle: {}", err);
                return Err(Processor {
                    receiver: self.receiver,
                    exporter: self.exporter,
                    start_time: self.start_time,
                    id: self.id,
                    state: Failed {
                        runtime: self.start_time.elapsed().as_millis(),
                        error: err.to_string(),
                        report: None,
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

        let link_target =
            KibanaLinkTarget::for_diagnostic(identifiers.platform.unwrap_or_default(), report.diagnostic.application);
        if let Some(kibana_link) = self
            .exporter
            .resolved_kibana_link(
                link_target,
                &report.diagnostic.metadata.id,
                report.diagnostic.metadata.collection_date,
            )
            .await
        {
            report.add_kibana_link(kibana_link);
        }
        tracing::debug!("{:?}", identifiers);
        report.add_identifiers(identifiers);
        report.add_origin(origin);
        report.add_processing_duration(self.start_time.elapsed().as_millis());
        let mut report_error = match self.exporter.save_report(&report).await {
            Ok(()) => None,
            Err(error) => {
                let reason = format!("Failed to save diagnostic report: {error}");
                tracing::error!("{reason}");
                report.record_event(DiagnosticEvent::error("report", reason.clone()));
                Some(eyre!(reason))
            }
        };

        // Save the definitive outcome once, after the remote report attempt.
        // Keep its error primary if the local recovery record also cannot be saved.
        if matches!(self.exporter.as_ref(), Exporter::Elasticsearch(_))
            && let Err(local_error) = data::save_file("report.json", &report)
        {
            let reason = format!("Failed to save diagnostic report locally: {local_error}");
            tracing::error!("{reason}");
            report.record_event(DiagnosticEvent::error("report", reason.clone()));
            if report_error.is_none() {
                report_error = Some(eyre!(reason));
            }
        }

        if let Some(error) = process_error.or(report_error) {
            return Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: error.to_string(),
                    report: Some(report),
                },
            });
        }

        let output = self.exporter.outcome_uri();
        Ok(Processor {
            exporter: self.exporter,
            receiver: self.receiver,
            start_time: self.start_time,
            id: self.id,
            state: Completed {
                report,
                runtime: self.start_time.elapsed().as_millis(),
                output,
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
        match manifest.bundle_origin() {
            BundleOrigin::Platform(Platform::ECK) => {
                let (diagnostic, report) =
                    ElasticCloudKubernetesDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::ElasticCloudKubernetes(diagnostic), report))
            }
            BundleOrigin::Platform(Platform::KubernetesPlatform) => {
                let (diagnostic, report) =
                    KubernetesPlatformDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::KubernetesPlatform(diagnostic), report))
            }
            BundleOrigin::Application(Application::Elasticsearch) => {
                let (diagnostic, report) =
                    ElasticsearchDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Elasticsearch(diagnostic), report))
            }
            BundleOrigin::Application(Application::Logstash) => {
                let (diagnostic, report) =
                    LogstashDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Logstash(diagnostic), report))
            }
            BundleOrigin::Application(Application::Kibana) => {
                let (diagnostic, report) =
                    KibanaDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Kibana(diagnostic), report))
            }
            BundleOrigin::Application(Application::Agent) => Err(eyre!(AGENT_PROCESSING_NOT_IMPLEMENTED)),
            BundleOrigin::Platform(_) | BundleOrigin::Unknown => Err(eyre!(UNSUPPORTED_PRODUCT_OR_DIAGNOSTIC_BUNDLE)),
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
    use std::{fs::File, path::Path, sync::Arc};
    use url::Url;
    use zip::ZipArchive;

    fn archive_path(name: &str) -> String {
        format!("{}/../../tests/archives/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn extract_archive(name: &str, destination: &Path) {
        std::fs::create_dir_all(destination).expect("create child diagnostic dir");
        let file = File::open(archive_path(name)).expect("open fixture archive");
        let mut archive = ZipArchive::new(file).expect("read fixture archive");
        archive.extract(destination).expect("extract fixture archive");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn kibana_processing() {
        let root = tempfile::tempdir().expect("bundle directory");
        extract_archive("kibana-api-diagnostics-8.19.3.zip", root.path());

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output directory");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default(), None)
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
    async fn syscalls_folder_implies_self_managed_platform() {
        let root = tempfile::tempdir().expect("bundle dir");
        extract_archive("elasticsearch-api-diagnostics-9.3.3.zip", root.path());
        std::fs::create_dir_all(root.path().join("syscalls")).expect("create syscalls dir");
        let manifest_path = root.path().join(DiagnosticManifest::FILENAME);
        let manifest_before = std::fs::read_to_string(&manifest_path).expect("read manifest before processing");

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let processor = Processor::try_new(receiver, exporter, Identifiers::default(), None)
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
        let processor = Processor::try_new(receiver, exporter, Identifiers::default(), None)
            .await
            .expect("ready processor");

        assert_eq!(processor.state.manifest.platform(), Platform::Unknown);
        assert_eq!(processor.state.identifiers.platform, Some(Platform::Unknown));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cloud_admin_receiver_implies_elastic_cloud_hosted_platform() {
        let host = KnownHostBuilder::new(Url::parse("https://admin.found.no").expect("url"))
            .auth(crate::data::Auth::apikey("test-api-key"))
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
        let processor = Processor::try_new(receiver, exporter, Identifiers::default(), None)
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

    const SAR_TEXT: &str = "Linux 6.1.0 (node-01.example.com) \t2026-08-05 \t_x86_64_\t(2 CPU)\n\n12:00:01 AM CPU %usr %idle\n12:10:10 AM all 6.10 90.00\n12:10:10 AM 0 5.00 91.00\n";

    struct SyscallRun {
        report: DiagnosticReport,
        output: tempfile::TempDir,
    }

    impl SyscallRun {
        fn sar_events(&self) -> Vec<(EventSeverity, String)> {
            self.report
                .events()
                .iter()
                .filter(|event| event.source == "syscalls.sar")
                .map(|event| (event.severity, event.reason.clone()))
                .collect()
        }

        fn sar_docs(&self) -> usize {
            std::fs::read_to_string(self.output.path().join("metrics-host.sar-esdiag.ndjson"))
                .map(|docs| docs.lines().filter(|line| !line.trim().is_empty()).count())
                .unwrap_or(0)
        }
    }

    async fn process_es_bundle(files: &[(&str, &[u8])], requested: Option<&[&str]>) -> SyscallRun {
        let root = tempfile::tempdir().expect("bundle dir");
        extract_archive("elasticsearch-api-diagnostics-9.3.3.zip", root.path());
        for (path, contents) in files {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            std::fs::write(path, contents).expect("write bundle file");
        }
        let selection = requested.map(|keys| {
            let keys: Vec<String> = keys.iter().map(|key| key.to_string()).collect();
            api::ApiResolver::process_selection("elasticsearch", "standard", &keys).expect("selection")
        });

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(root.path().to_path_buf())).expect("receiver"));
        let output = tempfile::tempdir().expect("output dir");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let completed = Processor::try_new(receiver, exporter, Identifiers::default(), selection)
            .await
            .expect("ready processor")
            .start()
            .await
            .map_err(|failed| failed.state.error)
            .expect("processing processor")
            .process()
            .await
            .map_err(|failed| failed.state.error)
            .expect("completed processor");
        SyscallRun {
            report: completed.state.report,
            output,
        }
    }

    fn warnings(events: &[(EventSeverity, String)]) -> Vec<&str> {
        events
            .iter()
            .filter(|(severity, _)| *severity == EventSeverity::Warning)
            .map(|(_, reason)| reason.as_str())
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bundle_without_syscalls_records_no_sar_event() {
        let run = process_es_bundle(&[], None).await;
        assert!(run.sar_events().is_empty());
        assert_eq!(run.sar_docs(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn binary_sysstat_files_alone_are_silently_missing() {
        let run = process_es_bundle(&[("syscalls/sa05", b"\x96\xd5\x71\x21\x00\x00binary")], None).await;
        assert!(run.sar_events().is_empty());
        assert_eq!(run.sar_docs(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn explicitly_requested_missing_sar_warns_without_failing() {
        let run = process_es_bundle(
            &[("syscalls/ss.txt", b"Netid State\n")],
            Some(&["nodes", "syscalls.sar"]),
        )
        .await;
        let events = run.sar_events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(warnings(&events)[0].contains("syscalls/sar.txt"));
        assert!(run.report.diagnostic.docs.created > 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn selection_omitting_sar_leaves_the_file_unread() {
        let run = process_es_bundle(&[("syscalls/sar.txt", SAR_TEXT.as_bytes())], Some(&["nodes"])).await;
        assert!(run.sar_events().is_empty());
        assert_eq!(run.sar_docs(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn default_selection_processes_sar_with_a_success_verdict() {
        let run = process_es_bundle(&[("syscalls/sar.txt", SAR_TEXT.as_bytes())], None).await;
        assert_eq!(run.sar_docs(), 2);
        let events = run.sar_events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].0, EventSeverity::Success);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sar_without_a_report_header_warns_in_every_mode() {
        let files: &[(&str, &[u8])] = &[("syscalls/sar.txt", b"bash: sar: command not found\n")];
        for requested in [None, Some(&["nodes", "syscalls.sar"][..])] {
            let run = process_es_bundle(files, requested).await;
            let events = run.sar_events();
            assert_eq!(events.len(), 1, "{events:?}");
            assert!(warnings(&events)[0].contains("command not found"), "{events:?}");
            assert_eq!(run.sar_docs(), 0);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unknown_sar_sections_warn_beside_a_success_verdict() {
        let text = format!("{SAR_TEXT}\n12:00:01 AM widgets/s gadgets/s\n12:10:10 AM 1.00 2.00\n");
        let run = process_es_bundle(&[("syscalls/sar.txt", text.as_bytes())], None).await;
        let events = run.sar_events();
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events.iter().any(|(severity, _)| *severity == EventSeverity::Success));
        assert!(warnings(&events)[0].contains("widgets/s gadgets/s"));
        assert_eq!(run.sar_docs(), 2);
    }
}
