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

pub use collector::{CollectionResult, Collector, default_collect_archive_name};
pub use diagnostic::{
    DataSource, DiagnosticManifest, DiagnosticReport, Manifest, RequestedApi, SourceContext,
    data_source::init_sources,
    manifest::ManifestBuilder,
    report::{BatchResponse, Identifiers, ProcessorSummary},
};
pub use elasticsearch::Cluster as ElasticsearchCluster;

pub use crate::processor::diagnostic::data_source::StreamingDataSource;
use crate::{data::Product, exporter::Exporter, receiver::Receiver};
use api::ProcessSelection;
use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::ElasticsearchDiagnostic;
use eyre::{Result, eyre};
use futures::{
    FutureExt,
    future::BoxFuture,
    stream::{BoxStream, FuturesUnordered},
};
use kubernetes_platform::KubernetesPlatformDiagnostic;
use logstash::LogstashDiagnostic;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::{sync::mpsc, time::Instant};

pub struct Processor<S: State> {
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
    pub included_diagnostics: Vec<IncludedDiagnosticOutcome>,
}

/// The `Failed` state represents a failed processing job
pub struct Failed {
    pub error: String,
    pub runtime: u128,
}

/// The `Status` trait represents the state of a processing job
pub trait State {}
// The Status trait doesn't need any functions, it is only used for trait bounds
impl State for Ready {}
impl State for Processing {}
impl State for Completed {}
impl State for Failed {}

pub enum IncludedDiagnosticOutcome {
    Completed {
        job_id: u64,
        path: String,
        report: Box<DiagnosticReport>,
        runtime: u128,
    },
    Skipped {
        job_id: u64,
        path: String,
        product: Option<Product>,
        reason: String,
    },
    Failed {
        job_id: u64,
        path: String,
        error: String,
    },
}

impl IncludedDiagnosticOutcome {
    pub fn job_id(&self) -> u64 {
        match self {
            Self::Completed { job_id, .. } | Self::Skipped { job_id, .. } | Self::Failed { job_id, .. } => *job_id,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Self::Completed { path, .. } | Self::Skipped { path, .. } | Self::Failed { path, .. } => path,
        }
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
        product: Product,
        diagnostic_id: String,
        docs_created: u32,
        duration_ms: u128,
        kibana_link: Option<String>,
    },
    Skipped {
        job_id: u64,
        path: String,
        product: Option<Product>,
        reason: String,
    },
    Failed {
        job_id: u64,
        path: String,
        error: String,
    },
}

fn spawn_sub_processors(
    parent_job_id: u64,
    diag_paths: Vec<diagnostic::DiagPath>,
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    identifiers: Option<Identifiers>,
    child_event_tx: Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
) -> FuturesUnordered<BoxFuture<'static, IncludedDiagnosticOutcome>> {
    let handles = FuturesUnordered::new();
    let identifiers = identifiers.unwrap_or_default();
    for (index, diag_path) in diag_paths.into_iter().enumerate() {
        let child_job_id = child_job_id(parent_job_id, index);
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
                    let outcome = IncludedDiagnosticOutcome::Failed {
                        job_id: child_job_id,
                        path,
                        error: format!("Failed to clone receiver for included diagnostic: {e}"),
                    };
                    send_child_outcome_event(&event_tx, &outcome);
                    return outcome;
                }
            };

            let processor = match Processor::try_new_child(receiver, exporter, ident_clone).await {
                Ok(processor) => processor,
                Err(e) => {
                    let outcome = IncludedDiagnosticOutcome::Failed {
                        job_id: child_job_id,
                        path,
                        error: format!("Failed to read included diagnostic manifest: {e}"),
                    };
                    send_child_outcome_event(&event_tx, &outcome);
                    return outcome;
                }
            };

            let product = processor.state.manifest.product.clone();
            match processor.start().await {
                Ok(processing) => match processing.process().await {
                    Ok(complete) => {
                        tracing::info!("Included diagnostic processor complete");
                        let outcome = IncludedDiagnosticOutcome::Completed {
                            job_id: child_job_id,
                            path,
                            report: Box::new(complete.state.report),
                            runtime: complete.state.runtime,
                        };
                        send_child_outcome_event(&event_tx, &outcome);
                        outcome
                    }
                    Err(failed) => {
                        let outcome = IncludedDiagnosticOutcome::Failed {
                            job_id: child_job_id,
                            path,
                            error: failed.state.error,
                        };
                        send_child_outcome_event(&event_tx, &outcome);
                        outcome
                    }
                },
                Err(failed) if is_unsupported_child_processor(&failed.state.error) => {
                    let outcome = IncludedDiagnosticOutcome::Skipped {
                        job_id: child_job_id,
                        path,
                        product: Some(product),
                        reason: failed.state.error,
                    };
                    send_child_outcome_event(&event_tx, &outcome);
                    outcome
                }
                Err(failed) => {
                    let outcome = IncludedDiagnosticOutcome::Failed {
                        job_id: child_job_id,
                        path,
                        error: failed.state.error,
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
                        let outcome = IncludedDiagnosticOutcome::Failed {
                            job_id: child_job_id,
                            path: join_path,
                            error: format!("Included diagnostic task failed to join: {e}"),
                        };
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

fn send_child_event(
    child_event_tx: &Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
    event: IncludedDiagnosticJobEvent,
) {
    if let Some(tx) = child_event_tx {
        let _ = tx.send(event);
    }
}

fn send_child_outcome_event(
    child_event_tx: &Option<mpsc::UnboundedSender<IncludedDiagnosticJobEvent>>,
    outcome: &IncludedDiagnosticOutcome,
) {
    let event = match outcome {
        IncludedDiagnosticOutcome::Completed {
            job_id,
            path,
            report,
            runtime,
        } => IncludedDiagnosticJobEvent::Completed {
            job_id: *job_id,
            path: path.clone(),
            product: report.diagnostic.product.clone(),
            diagnostic_id: report.diagnostic.metadata.id.clone(),
            docs_created: report.diagnostic.docs.created,
            duration_ms: *runtime,
            kibana_link: report.diagnostic.kibana_link.clone(),
        },
        IncludedDiagnosticOutcome::Skipped {
            job_id,
            path,
            product,
            reason,
        } => IncludedDiagnosticJobEvent::Skipped {
            job_id: *job_id,
            path: path.clone(),
            product: product.clone(),
            reason: reason.clone(),
        },
        IncludedDiagnosticOutcome::Failed { job_id, path, error } => IncludedDiagnosticJobEvent::Failed {
            job_id: *job_id,
            path: path.clone(),
            error: error.clone(),
        },
    };
    send_child_event(child_event_tx, event);
}

fn is_unsupported_child_processor(error: &str) -> bool {
    error.contains("processing is not yet implemented") || error.contains("Unsupported product or diagnostic bundle")
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
        let manifest = receiver.try_get_manifest().await?;
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

    async fn try_new_child(receiver: Arc<Receiver>, exporter: Arc<Exporter>, identifiers: Identifiers) -> Result<Self> {
        Self::try_new_with_options(receiver, exporter, identifiers, None, None, false).await
    }
}

impl Processor<Ready> {
    pub async fn try_new_with_child_events(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
        child_event_tx: mpsc::UnboundedSender<IncludedDiagnosticJobEvent>,
    ) -> Result<Self> {
        Self::try_new_with_options(
            receiver,
            exporter,
            identifiers,
            process_selection,
            Some(child_event_tx),
            true,
        )
        .await
    }
}

impl Processor<Ready> {
    /// Try creating a processor with the receiver, exporter and identifiers.
    /// Will attempt to build a manifest from a call to the receiver.
    pub async fn try_new(receiver: Arc<Receiver>, exporter: Arc<Exporter>, identifiers: Identifiers) -> Result<Self> {
        Self::try_new_with_selection(receiver, exporter, identifiers, None).await
    }

    pub async fn try_new_with_selection(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
        process_selection: Option<ProcessSelection>,
    ) -> Result<Self> {
        Self::try_new_with_options(receiver, exporter, identifiers, process_selection, None, true).await
    }

    /// State transition from `Ready` to `Processing`, returning the progress channel
    pub async fn start(self) -> Result<Processor<Processing>, Processor<Failed>> {
        tracing::debug!("Transitioned: Processor<Processing>");
        let (summary_tx, summary_rx) = mpsc::channel::<ProcessorSummary>(10);

        let mut identifiers = self.state.identifiers.clone();
        if identifiers.orchestration.is_none() {
            let orchestration = match self.state.manifest.product {
                Product::ECK => Some("elastic-cloud-kubernetes".to_string()),
                Product::ECE => Some("elastic-cloud-enterprise".to_string()),
                Product::KubernetesPlatform => Some("kubernetes-platform".to_string()),
                Product::ElasticCloudHosted => Some("elastic-cloud-hosted".to_string()),
                _ => None,
            };
            if let Some(orch) = orchestration {
                identifiers = identifiers.with_orchestration(orch);
            }
        }

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
                        },
                    });
                }
            };

            let mut child_identifiers = identifiers.clone();
            if let Some(parent_uuid) = diagnostic.uuid() {
                child_identifiers = child_identifiers.with_parent_id(parent_uuid);
            }

            let sub_processors = if self.state.process_included_diagnostics {
                spawn_sub_processors(
                    self.id,
                    included_diagnostics,
                    self.receiver.clone(),
                    self.exporter.clone(),
                    Some(child_identifiers),
                    self.child_event_tx.clone(),
                )
            } else {
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
                },
            }),
        }
    }
}

/// The actively `Processing` state.
impl Processor<Processing> {
    #[tracing::instrument(skip_all)]
    pub async fn process(self) -> Result<Processor<Completed>, Processor<Failed>> {
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

        let process_result = diagnostic.process(summary_tx).await;
        if let Err(err) = process_result {
            return Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                child_event_tx: self.child_event_tx,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: err.to_string(),
                },
            });
        }

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
                    },
                });
            }
        };

        tracing::info!(
            "Created {} documents for {} diagnostic: {}",
            report.diagnostic.docs.created,
            report.diagnostic.product,
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
        if let Err(e) = self.exporter.save_report(&report).await {
            tracing::error!("Failed to save report: {}", e);
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
    //Kibana(KibanaDiagnostic)
    Logstash(Box<LogstashDiagnostic>),
}

impl Diagnostic {
    pub fn uuid(&self) -> Option<String> {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::ElasticCloudKubernetes(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::KubernetesPlatform(diagnostic) => Some(diagnostic.uuid().to_string()),
            Diagnostic::Logstash(diagnostic) => Some(diagnostic.uuid().to_string()),
        }
    }

    pub async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Self, DiagnosticReport)> {
        tracing::info!("Processing {} diagnostic", manifest.product);
        tracing::trace!("Diagnostic Manifest: {}", serde_json::to_string(&manifest).unwrap());
        if let Some(selection) = &process_selection
            && product_key(&manifest.product) != selection.product
        {
            return Err(eyre!(
                "Selected processing product '{}' does not match diagnostic product '{}'",
                selection.product,
                product_key(&manifest.product)
            ));
        }
        match manifest.product {
            Product::Elasticsearch => {
                let (diagnostic, report) =
                    ElasticsearchDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Elasticsearch(diagnostic), report))
            }
            Product::ECK => {
                let (diagnostic, report) =
                    ElasticCloudKubernetesDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::ElasticCloudKubernetes(diagnostic), report))
            }
            Product::KubernetesPlatform => {
                let (diagnostic, report) =
                    KubernetesPlatformDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::KubernetesPlatform(diagnostic), report))
            }
            Product::Logstash => {
                let (diagnostic, report) =
                    LogstashDiagnostic::try_new(receiver, exporter, manifest, process_selection).await?;
                Ok((Self::Logstash(diagnostic), report))
            }
            Product::Kibana => Err(eyre!("Kibana processing is not yet implemented")),
            _ => Err(eyre!("Unsupported product or diagnostic bundle")),
        }
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::ElasticCloudKubernetes(diagnostic) => diagnostic.process(summary_tx).await,
            Diagnostic::KubernetesPlatform(diagnostic) => diagnostic.process(summary_tx).await,
            //Diagnostic::Kibana(diagnostic) => diagnostic.run().await?,
            Diagnostic::Logstash(diagnostic) => diagnostic.process(summary_tx).await,
        }
    }

    fn origin(&self) -> (String, String, String) {
        match self {
            Diagnostic::Elasticsearch(diagnostic) => diagnostic.origin(),
            Diagnostic::ElasticCloudKubernetes(diagnostic) => diagnostic.origin(),
            Diagnostic::KubernetesPlatform(diagnostic) => diagnostic.origin(),
            //Diagnostic::Kibana(diagnostic) => diagnostic.origin(),
            Diagnostic::Logstash(diagnostic) => diagnostic.origin(),
        }
    }
}

trait DocumentExporter<T, U> {
    async fn documents_export(self, exporter: &Exporter, lookups: &T, metadata: &U) -> ProcessorSummary;
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

fn product_key(product: &Product) -> String {
    match product {
        Product::Elasticsearch => "elasticsearch".to_string(),
        Product::Logstash => "logstash".to_string(),
        Product::KubernetesPlatform => "kubernetes-platform".to_string(),
        Product::ECK => "elastic-cloud-kubernetes".to_string(),
        Product::ECE => "elastic-cloud-enterprise".to_string(),
        Product::ElasticCloudHosted => "elastic-cloud-hosted".to_string(),
        Product::Kibana => "kibana".to_string(),
        other => other.to_string().to_lowercase(),
    }
}

trait Metadata {
    fn as_meta_doc(&self) -> serde_json::Value;
}

pub fn new_job_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        % 100000
}

fn child_job_id(parent_job_id: u64, index: usize) -> u64 {
    (parent_job_id << 32) | (index as u64 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{Product, Uri},
        exporter::Exporter,
        receiver::Receiver,
    };
    use serde_json::json;
    use std::collections::HashSet;
    use std::{fs::File, path::Path, sync::Arc};
    use tempfile::TempDir;
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
        for outcome in &completed.state.included_diagnostics {
            let IncludedDiagnosticOutcome::Completed { report, .. } = outcome else {
                panic!("expected supported child to complete");
            };
            assert_eq!(report.diagnostic.product, Product::Elasticsearch);
            assert!(report.diagnostic.docs.created > 0);
            assert!(report.diagnostic.identifiers.parent_id.is_some());
            assert_eq!(
                report.diagnostic.identifiers.orchestration.as_deref(),
                Some("elastic-cloud-kubernetes")
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unsupported_readable_child_returns_skipped_outcome() {
        let root = parent_with_children(&[("kibana-child", "kibana-api-diagnostics-9.3.3.zip")]);

        let completed = process_parent_bundle(root.path()).await;

        assert_eq!(completed.state.included_diagnostics.len(), 1);
        let IncludedDiagnosticOutcome::Skipped { product, reason, .. } = &completed.state.included_diagnostics[0]
        else {
            panic!("expected unsupported child to be skipped");
        };
        assert_eq!(product.as_ref(), Some(&Product::Kibana));
        assert!(reason.contains("Kibana processing is not yet implemented"));
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

        assert_eq!(completed.state.report.diagnostic.product, Product::ECK);
        assert_eq!(completed.state.included_diagnostics.len(), 1);
        let IncludedDiagnosticOutcome::Failed { path, error, .. } = &completed.state.included_diagnostics[0] else {
            panic!("expected unreadable child to fail independently");
        };
        assert_eq!(path, "missing-child");
        assert!(error.contains("Failed to read included diagnostic manifest"));
    }

    #[test]
    fn child_job_ids_do_not_overlap_across_parent_ranges() {
        assert_ne!(child_job_id(2, 0), child_job_id(1, 1000));
    }
}
