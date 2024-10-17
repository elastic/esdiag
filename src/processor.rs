/// Processors for Elasticsearch diagnostics
pub mod elasticsearch;
/// Lookup processors
mod lookup;

use std::sync::Arc;

use elasticsearch::{ElasticsearchDiagnostic, Lookups};

use crate::{
    data::diagnostic::{DiagnosticManifest, Product},
    exporter::Exporter,
    receiver::Receiver,
};
use color_eyre::eyre::{eyre, Result};

pub enum Diagnostic {
    Elasticsearch(Box<ElasticsearchDiagnostic>),
    //Kibana(KibanaDiagnostic)
    //Logstash(LogstashDiagnostic)
}

impl Diagnostic {
    pub async fn try_new_processor(
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
            _ => Err(eyre!("Unsupported product or diagnostic bundle")),
        }
    }

    pub async fn run(self) -> Result<(String, usize)> {
        match self {
            Self::Elasticsearch(diagnostic) => diagnostic.run().await,
            //Self::Kibana(diagnostic) => diagnostic.run().await,
            //Self::Logstash(diagnostic) => diagnostic.run().await,
        }
    }
}

trait DataProcessor<T> {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<T>,
    ) -> (String, Vec<serde_json::Value>);
}

trait DiagnosticProcessor {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>>;
    async fn process_queue(&self) -> usize;
    async fn run(self) -> Result<(String, usize)>;
}

trait Metadata {
    fn as_meta_doc(&self) -> serde_json::Value;
}
