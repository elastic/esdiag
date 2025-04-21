/// Processors for Elastic Cloud Kubernetes (ECK) diagnostics
pub mod elastic_cloud_kubernetes;
/// Processors for Elasticsearch diagnostics
pub mod elasticsearch;
/// Processors for Managed Kubernetes Infrastructure (MKI) platform diagnostics
pub mod kubernetes_platform;
/// Processors for Logstash diagnostics
pub mod logstash;

use std::sync::Arc;

use elastic_cloud_kubernetes::ElasticCloudKubernetesDiagnostic;
use elasticsearch::ElasticsearchDiagnostic;
use kubernetes_platform::KubernetesPlatformDiagnostic;
use logstash::LogstashDiagnostic;

use crate::{
    data::diagnostic::{DiagnosticManifest, Product},
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::{Result, eyre};

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

    pub async fn run(self) -> Result<()> {
        match self {
            Self::Elasticsearch(diagnostic) => diagnostic.run().await?,
            Self::ElasticCloudKubernetes(diagnostic) => diagnostic.run().await?,
            Self::KubernetesPlatform(diagnostic) => diagnostic.run().await?,
            //Self::Kibana(diagnostic) => diagnostic.run().await?,
            Self::Logstash(diagnostic) => diagnostic.run().await?,
        };
        Ok(())
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
    async fn run(self) -> Result<()>;
}

trait Metadata {
    fn as_meta_doc(&self) -> serde_json::Value;
}
