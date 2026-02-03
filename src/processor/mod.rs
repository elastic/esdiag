// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Collect diagnostic data from applications
mod collector;
/// Universal diagnostic processor
mod diagnostic;
/// Processors for Elastic Cloud Kubernetes (ECK) diagnostics
mod elastic_cloud_kubernetes;
/// Processors for Elasticsearch diagnostics
mod elasticsearch;
/// Processors for Managed Kubernetes Infrastructure (MKI) platform diagnostics
mod kubernetes_platform;
/// Processors for Logstash diagnostics
mod logstash;
pub use collector::Collector;
pub use diagnostic::{
    DataSource, DiagnosticManifest, DiagnosticReport, Manifest,
    data_source::PathType,
    manifest::ManifestBuilder,
    report::{BatchResponse, Identifiers, ProcessorSummary},
};
pub use elasticsearch::Cluster as ElasticsearchCluster;
use futures::stream::FuturesUnordered;

use crate::{data::Product, exporter::Exporter, receiver::Receiver};
use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::ElasticsearchDiagnostic;
use eyre::{Result, eyre};
use kubernetes_platform::KubernetesPlatformDiagnostic;
use logstash::LogstashDiagnostic;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::{sync::mpsc, task::JoinHandle, time::Instant};

pub struct Processor<S: State> {
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    pub start_time: Instant,
    pub id: u64,
    pub state: S,
}

/// The `Ready` state represents a new processing job
pub struct Ready {
    manifest: DiagnosticManifest,
    identifiers: Identifiers,
}

/// The `Processing` state represents an active processing job
pub struct Processing {
    diagnostic: Diagnostic,
    identifiers: Identifiers,
    summary_tx: mpsc::Sender<ProcessorSummary>,
    summary_rx: mpsc::Receiver<ProcessorSummary>,
    report: DiagnosticReport,
}

/// The `Completed` state represents a succesfull processing job
pub struct Completed {
    pub report: DiagnosticReport,
    pub runtime: u128,
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

fn spawn_sub_processors(
    diag_paths: Vec<diagnostic::DiagPath>,
    receiver: Arc<Receiver>,
    exporter: Arc<Exporter>,
    identifiers: Option<Identifiers>,
) -> FuturesUnordered<JoinHandle<()>> {
    let handles = FuturesUnordered::new();
    let identifiers = identifiers.unwrap_or_default();
    for diag_path in diag_paths {
        let receiver = match receiver.clone_for_subdir(&diag_path.diag_path) {
            Ok(receiver) => receiver,
            Err(e) => {
                log::error!("Failed to clone receiver for sub-processor: {} ", e);
                continue;
            }
        };
        let exporter = exporter.clone();
        let ident_clone = identifiers.clone();
        let handle = tokio::spawn(async move {
            match Processor::try_new(Arc::new(receiver), exporter, ident_clone).await {
                Ok(processor) => {
                    match processor.start().await {
                        Ok(processing) => match processing.process().await {
                            Ok(_complete) => {
                                log::info!("Sub-processor complete");
                            }
                            Err(failed) => {
                                log::error!("Sub-processor failed: {}", failed);
                            }
                        },
                        Err(failed) => {
                            log::error!("Sub-processor failed: {}", failed);
                        }
                    };
                }
                Err(e) => {
                    log::error!("Diagnostic sub-processor failed: {}", e);
                }
            };
        });
        handles.push(handle)
    }
    handles
}

impl Processor<Ready> {
    /// Try creating a processor with the receiver, exporter and identifiers.
    /// Will attempt to build a manifest from a call to the receiver.
    pub async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        identifiers: Identifiers,
    ) -> Result<Self> {
        let manifest = receiver.try_get_manifest().await?;
        Ok(Self {
            receiver,
            exporter,
            id: new_job_id(),
            start_time: Instant::now(),
            state: Ready {
                manifest,
                identifiers,
            },
        })
    }

    /// State transition from `Ready` to `Processing`, returning the progress channel
    pub async fn start(self) -> Result<Processor<Processing>, Processor<Failed>> {
        log::debug!("Transitioned: Processor<Processing>");
        let (summary_tx, summary_rx) = mpsc::channel::<ProcessorSummary>(10);

        if let Some(included_diagnostics) = self.state.manifest.included_diagnostics.clone() {
            let handles = spawn_sub_processors(
                included_diagnostics,
                self.receiver.clone(),
                self.exporter.clone(),
                self.state.manifest.identifiers.clone(),
            );
            for handle in handles {
                match handle.await {
                    Ok(_) => log::debug!("Sub-process task complete"),
                    Err(_) => log::debug!("Sub-process task failed"),
                }
            }
        };

        match Diagnostic::try_new(
            self.receiver.clone(),
            self.exporter.clone(),
            self.state.manifest,
        )
        .await
        {
            Ok((diagnostic, report)) => {
                let processor = Processor {
                    receiver: self.receiver,
                    exporter: self.exporter,
                    id: self.id,
                    start_time: self.start_time,
                    state: Processing {
                        diagnostic,
                        identifiers: self.state.identifiers,
                        summary_rx,
                        summary_tx,
                        report,
                    },
                };
                Ok(processor)
            }
            Err(err) => Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
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
    pub async fn process(mut self) -> Result<Processor<Completed>, Processor<Failed>> {
        log::debug!("Processing with async progress updates");

        let mut report = self.state.report;
        let origin = self.state.diagnostic.origin();
        let summary_handle = tokio::spawn(async move {
            while let Some(summary) = self.state.summary_rx.recv().await {
                log::debug!("{}", summary);
                report.add_processor_summary(summary);
            }
            report
        });

        let process_result = self.state.diagnostic.process(self.state.summary_tx).await;
        if let Err(err) = process_result {
            return Err(Processor {
                receiver: self.receiver,
                exporter: self.exporter,
                start_time: self.start_time,
                id: self.id,
                state: Failed {
                    runtime: self.start_time.elapsed().as_millis(),
                    error: err.to_string(),
                },
            });
        }

        let mut report = match summary_handle.await {
            Ok(report) => report,
            Err(err) => {
                log::error!("Failed to await summary handle: {}", err);
                return Err(Processor {
                    receiver: self.receiver,
                    exporter: self.exporter,
                    start_time: self.start_time,
                    id: self.id,
                    state: Failed {
                        runtime: self.start_time.elapsed().as_millis(),
                        error: err.to_string(),
                    },
                });
            }
        };

        log::info!(
            "Created {} documents for {} diagnostic: {}",
            report.diagnostic.docs.created,
            report.diagnostic.product,
            report.diagnostic.metadata.id,
        );

        if let Ok(kibana_url) = std::env::var("ESDIAG_KIBANA_URL") {
            let kibana_space = match std::env::var("ESDIAG_KIBANA_SPACE") {
                Ok(space) => format!("/s/{space}"),
                Err(_) => String::from(""),
            };
            let url_safe_id = urlencoding::encode(&report.diagnostic.metadata.id);
            let days_since_collection = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
                - report.diagnostic.metadata.collection_date)
                / (1000 * 60 * 60 * 24);
            let time_filter = match days_since_collection {
                x if x < 90 => "from:now-90d,to:now",
                x if x >= 90 && x < 365 => "from:now-1y,to:now",
                x => &format!("from:now-{}d,to:now", x + 1),
            };
            let kibana_link = format!(
                "{}/app/dashboards#/view/elasticsearch-cluster-report?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'4319ebc4-df81-4b18-b8bd-6aaa55a1fd13',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:({}))",
                "{}{kibana_space}/app/dashboards#/view/elasticsearch-cluster-report?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'4319ebc4-df81-4b18-b8bd-6aaa55a1fd13',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:({}))",
                kibana_url, url_safe_id, url_safe_id, time_filter
            );
            log::info!("{}", kibana_link);
            report.add_kibana_link(kibana_link);
        }
        log::debug!("{:?}", self.state.identifiers);
        report.add_identifiers(self.state.identifiers);
        report.add_origin(origin);
        report.add_processing_duration(self.start_time.elapsed().as_millis());
        if let Err(e) = self.exporter.save_report(&report).await {
            log::error!("Failed to save report: {}", e);
        }

        Ok(Processor {
            exporter: self.exporter,
            receiver: self.receiver,
            start_time: self.start_time,
            id: self.id,
            state: Completed {
                report,
                runtime: self.start_time.elapsed().as_millis(),
            },
        })
    }
}

impl std::fmt::Display for Processor<Failed> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Processor {} failed: {}", self.id, self.state.error)
    }
}

pub enum Diagnostic {
    Elasticsearch(Box<ElasticsearchDiagnostic>),
    ElasticCloudKubernetes(Box<ElasticCloudKubernetesDiagnostic>),
    KubernetesPlatform(Box<KubernetesPlatformDiagnostic>),
    //Kibana(KibanaDiagnostic)
    Logstash(Box<LogstashDiagnostic>),
}

impl Diagnostic {
    pub async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
    ) -> Result<(Self, DiagnosticReport)> {
        log::info!("Processing {} diagnostic", manifest.product);
        log::trace!(
            "Diagnostic Manifest: {}",
            serde_json::to_string(&manifest).unwrap()
        );
        match manifest.product {
            Product::Elasticsearch => {
                let (diagnostic, report) =
                    ElasticsearchDiagnostic::try_new(receiver, exporter, manifest).await?;
                Ok((Self::Elasticsearch(diagnostic), report))
            }
            Product::ECK => {
                let (diagnostic, report) =
                    ElasticCloudKubernetesDiagnostic::try_new(receiver, exporter, manifest).await?;
                Ok((Self::ElasticCloudKubernetes(diagnostic), report))
            }
            Product::KubernetesPlatform => {
                let (diagnostic, report) =
                    KubernetesPlatformDiagnostic::try_new(receiver, exporter, manifest).await?;
                Ok((Self::KubernetesPlatform(diagnostic), report))
            }
            Product::Logstash => {
                let (diagnostic, report) =
                    LogstashDiagnostic::try_new(receiver, exporter, manifest).await?;
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
    async fn documents_export(
        self,
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
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        % 100000
}
