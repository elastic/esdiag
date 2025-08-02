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
    DataSource, DiagnosticManifest, DiagnosticReport, Manifest, Product,
    data_source::PathType,
    manifest::ManifestBuilder,
    report::{BatchResponse, Identifiers, ProcessorSummary},
};
pub use elasticsearch::Cluster as ElasticsearchCluster;

use crate::{exporter::Exporter, receiver::Receiver};
use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::ElasticsearchDiagnostic;
use eyre::{Result, eyre};
use kubernetes_platform::KubernetesPlatformDiagnostic;
use logstash::LogstashDiagnostic;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub enum Diagnostic {
    Elasticsearch(Box<ElasticsearchDiagnostic>),
    ElasticCloudKubernetes(Box<ElasticCloudKubernetesDiagnostic>),
    KubernetesPlatform(Box<KubernetesPlatformDiagnostic>),
    //Kibana(KibanaDiagnostic)
    Logstash(Box<LogstashDiagnostic>),
}

impl Diagnostic {
    pub async fn try_new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Self> {
        log::info!("Processing {} diagnostic", manifest.product);
        log::trace!(
            "Diagnostic Manifest: {}",
            serde_json::to_string(&manifest).unwrap()
        );
        match manifest.product {
            Product::Elasticsearch => {
                let diagnostic = ElasticsearchDiagnostic::new(manifest, receiver, exporter).await?;
                Ok(Self::Elasticsearch(diagnostic))
            }
            Product::ECK => {
                let diagnostic =
                    ElasticCloudKubernetesDiagnostic::new(manifest, receiver, exporter).await?;
                Ok(Self::ElasticCloudKubernetes(diagnostic))
            }
            Product::KubernetesPlatform => {
                let diagnostic =
                    KubernetesPlatformDiagnostic::new(manifest, receiver, exporter).await?;
                Ok(Self::KubernetesPlatform(diagnostic))
            }
            Product::Logstash => {
                let diagnostic = LogstashDiagnostic::new(manifest, receiver, exporter).await?;
                Ok(Self::Logstash(diagnostic))
            }
            _ => Err(eyre!("Unsupported product or diagnostic bundle")),
        }
    }

    pub async fn run(self) -> Result<DiagnosticReport> {
        let mut report = match self {
            Self::Elasticsearch(diagnostic) => diagnostic.run().await?,
            Self::ElasticCloudKubernetes(diagnostic) => diagnostic.run().await?,
            Self::KubernetesPlatform(diagnostic) => diagnostic.run().await?,
            //Self::Kibana(diagnostic) => diagnostic.run().await?,
            Self::Logstash(diagnostic) => diagnostic.run().await?,
        };
        log::info!(
            "Created {} documents for {} diagnostic: {}",
            report.docs.created,
            report.product,
            report.metadata.id,
        );
        if let Ok(kibana_url) = std::env::var("ESDIAG_KIBANA_URL") {
            let kibana_link = format!(
                "{}/app/dashboards#/view/4e0a26b2-e5f8-4b58-b617-86f5cdd0edad?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:'4319ebc4-df81-4b18-b8bd-6aaa55a1fd13',key:diagnostic.id,negate:!f,params:(query:'{}'),type:phrase),query:(match_phrase:(diagnostic.id:'{}')))),refreshInterval:(pause:!t,value:60000),time:(from:now-7d,to:now))",
                kibana_url, report.metadata.id, report.metadata.id
            );
            log::info!("{}", kibana_link);
            report.add_kibana_link(kibana_link);
        }
        Ok(report)
    }
}

trait DataProcessor<T, U> {
    fn generate_docs(self, lookups: Arc<T>, metadata: Arc<U>) -> (String, Vec<serde_json::Value>);
}

trait DiagnosticProcessor {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>>;
    async fn run(self) -> Result<DiagnosticReport>;
}

trait Metadata {
    fn as_meta_doc(&self) -> serde_json::Value;
}

#[derive(Serialize)]
pub struct JobNew {
    pub id: String,
    filename: String,
    user: Option<String>,
    #[serde(skip_serializing)]
    receiver: Receiver,
}

#[derive(Serialize)]
pub struct JobReady {
    pub id: String,
    filename: String,
    user: Option<String>,
    #[serde(skip_serializing)]
    diagnostic: Diagnostic,
}

#[derive(Clone, Serialize)]
pub struct JobProcessing {
    pub id: String,
    filename: String,
    user: Option<String>,
    #[serde(skip_serializing)]
    diagnostic: Diagnostic,
}

#[derive(Serialize)]
pub struct JobCompleted {
    pub id: String,
    filename: String,
    user: Option<String>,
    report: DiagnosticReport,
}

#[derive(Serialize)]
pub struct JobFailed {
    pub id: String,
    pub filename: String,
    pub user: Option<String>,
    pub error: String,
}

impl From<String> for JobFailed {
    fn from(error: String) -> Self {
        JobFailed {
            id: uuid::Uuid::new_v4().to_string(),
            filename: String::new(),
            user: None,
            error,
        }
    }
}

#[derive(Serialize)]
pub enum Job {
    New(JobNew),
    Ready(JobReady),
    Processing(JobProcessing),
    Completed(JobCompleted),
    Failed(JobFailed),
}

impl JobNew {
    pub fn with_filename(self, filename: String) -> Self {
        JobNew { filename, ..self }
    }

    pub async fn ready(self, exporter: Exporter) -> Result<JobReady, JobFailed> {
        let manifest = match self.receiver.try_get_manifest().await {
            Ok(manifest) => manifest,
            Err(err) => {
                log::error!("Failed to build manifest: {}", err);
                return Err(JobFailed {
                    id: self.id,
                    filename: self.filename,
                    user: self.user,
                    error: err.to_string(),
                });
            }
        };

        match Diagnostic::try_new(manifest, self.receiver, exporter).await {
            Ok(diagnostic) => Ok(JobReady {
                id: self.id,
                filename: self.filename,
                user: self.user,
                diagnostic,
            }),
            Err(err) => Err(JobFailed {
                id: self.id,
                filename: self.filename,
                user: self.user,
                error: err.to_string(),
            }),
        }
    }
}

impl JobNew {
    pub fn new(filename: String, user: Option<String>, receiver: Receiver) -> Self {
        let id = Uuid::new_v4().to_string();
        JobNew {
            id,
            filename,
            user,
            receiver,
        }
    }
}

impl JobReady {
    pub fn start(self) -> JobProcessing {
        JobProcessing {
            id: self.id,
            filename: self.filename,
            user: self.user,
            diagnostic: self.diagnostic,
        }
    }
}

impl JobProcessing {
    pub async fn process(self) -> Result<JobCompleted, JobFailed> {
        match self.diagnostic.run().await {
            Ok(report) => Ok(JobCompleted {
                id: self.id,
                filename: self.filename,
                user: self.user,
                report,
            }),
            Err(error) => Err(JobFailed {
                id: self.id,
                filename: self.filename,
                user: self.user,
                error: error.to_string(),
            }),
        }
    }
}

impl Job {
    pub fn new(filename: String, user: Option<String>, receiver: Receiver) -> Self {
        let id = Uuid::new_v4().to_string();
        Job::New(JobNew {
            id,
            filename,
            user,
            receiver,
        })
    }

    pub fn id(&self) -> String {
        match self {
            Job::New(job) => job.id.clone(),
            Job::Ready(job) => job.id.clone(),
            Job::Processing(job) => job.id.clone(),
            Job::Completed(job) => job.id.clone(),
            Job::Failed(job) => job.id.clone(),
        }
    }

    pub fn filename(&self) -> String {
        match self {
            Job::New(job) => job.filename.clone(),
            Job::Ready(job) => job.filename.clone(),
            Job::Processing(job) => job.filename.clone(),
            Job::Completed(job) => job.filename.clone(),
            Job::Failed(job) => job.filename.clone(),
        }
    }

    pub fn user(&self) -> Option<String> {
        match self {
            Job::New(job) => job.user.clone(),
            Job::Ready(job) => job.user.clone(),
            Job::Processing(job) => job.user.clone(),
            Job::Completed(job) => job.user.clone(),
            Job::Failed(job) => job.user.clone(),
        }
    }

    pub async fn ready(self, exporter: Exporter) -> Result<Self, Self> {
        if let Job::New(job) = self {
            match job.ready(exporter).await {
                Ok(job) => Ok(Job::Ready(job)),
                Err(job) => Err(Job::Failed(job)),
            }
        } else {
            Err(Job::Failed(JobFailed {
                id: self.id(),
                filename: self.filename(),
                user: self.user(),
                error: "Attempted to ready a job that was not new".to_string(),
            }))
        }
    }
}
