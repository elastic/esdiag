// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Collector definition for Logstash diagnostics
mod collector;
/// Logstash hot threads
mod hot_threads;
/// Logstash diagnostic metadata
mod metadata;
/// Logstash node processor
mod node;
/// Logstash node stats processor
mod node_stats;
/// Logstash plugins
mod plugins;
/// Logstash version
mod version;

pub use collector::LogstashCollector;
pub use metadata::LogstashMetadata;

use super::{
    DiagnosticProcessor, DocumentExporter, Metadata, ProcessorSummary,
    diagnostic::{DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder},
};
use crate::{
    data::{self, Product},
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use node::Node;
use node_stats::NodeStats;
use plugins::Plugins;
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Lookups,
    metadata: LogstashMetadata,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl LogstashDiagnostic {
    async fn process_datasource<T>(
        &mut self,
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()>
    where
        T: DataSource
            + DocumentExporter<Lookups, LogstashMetadata>
            + DeserializeOwned
            + Send
            + Sync,
    {
        let data = self.receiver.get::<T>().await?;
        let summary = data
            .documents_export(&self.exporter, &self.lookups, &self.metadata)
            .await;
        summary_tx.send(summary).await.map_err(|err| {
            log::error!("Failed to send summary: {}", err);
            eyre!(err)
        })
    }

    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }
}

impl DiagnosticProcessor for LogstashDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let logstash_version = receiver.get::<version::Version>().await?;
        let metadata = LogstashMetadata::try_new(manifest, logstash_version)?;
        let plugins = receiver.get::<plugins::Plugins>().await?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .product(Product::Logstash)
            .receiver(receiver.to_string())
            .build()?;

        Ok((
            Box::new(Self {
                lookups: Lookups {
                    plugin_count: plugins.total,
                },
                receiver,
                exporter,
                metadata,
            }),
            report,
        ))
    }

    async fn process(mut self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        log::debug!("Running Logstash diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        self.process_datasource::<Node>(summary_tx.clone()).await?;
        self.process_datasource::<NodeStats>(summary_tx.clone())
            .await?;
        self.process_datasource::<Plugins>(summary_tx.clone())
            .await?;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            self.metadata.node.name.clone(),
            self.metadata.node.id.clone(),
            "node".to_string(),
        )
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub plugin_count: u32,
}
