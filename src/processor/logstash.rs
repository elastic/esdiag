/// Logstash diagnostic metadata
mod metadata;
/// Logstash node processor
mod node;
/// Logstash node stats processor
mod node_stats;
/// Logstash plugins
mod plugins;

use super::{DataProcessor, DiagnosticProcessor, Metadata};
use crate::{
    data::{
        self,
        diagnostic::{
            report::ProcessorSummary, DataSource, DiagnosticManifest, DiagnosticReport,
            DiagnosticReportBuilder, Product,
        },
        logstash::{Node, NodeStats, Plugins, Version},
    },
    exporter::Exporter,
    receiver::Receiver,
};
use color_eyre::eyre::Result;
use metadata::LogstashMetadata;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Arc<Lookups>,
    metadata: Arc<LogstashMetadata>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    report: Arc<RwLock<DiagnosticReport>>,
}

impl LogstashDiagnostic {
    async fn process<T>(&self) -> Result<ProcessorSummary>
    where
        T: DataSource + DataProcessor<Lookups, LogstashMetadata> + DeserializeOwned + Send + Sync,
    {
        match self
            .receiver
            .get::<T>()
            .await
            .map(|data| data.generate_docs(self.lookups.clone(), self.metadata.clone()))
        {
            Ok((index, docs)) => self.exporter.write(index, docs).await,
            Err(e) => Err(e.into()),
        }
    }
}

impl DiagnosticProcessor for LogstashDiagnostic {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
        let logstash_version = receiver.get::<Version>().await?;
        let metadata = LogstashMetadata::try_new(manifest, logstash_version)?;
        let plugins = receiver.get::<Plugins>().await?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .product(Product::Logstash)
            .build()?;

        Ok(Box::new(Self {
            lookups: Arc::new(Lookups {
                plugin_count: plugins.total,
            }),
            metadata: Arc::new(metadata),
            exporter: Arc::new(exporter),
            receiver: Arc::new(receiver),
            report: Arc::new(RwLock::new(report)),
        }))
    }

    async fn run(self) -> Result<DiagnosticReport> {
        log::debug!("Running Logstash diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let mut report = self.report.write().await;
        report.add_processor_summary(self.process::<Node>().await?);
        report.add_processor_summary(self.process::<NodeStats>().await?);
        report.add_processor_summary(self.process::<Plugins>().await?);

        Ok(report.clone())
    }
}

#[derive(Serialize)]
struct Lookups {
    plugin_count: u32,
}
