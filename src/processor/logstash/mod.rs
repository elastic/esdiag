// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

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

use super::{
    DataProcessor, DiagnosticProcessor, Metadata,
    diagnostic::{
        DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder, Product,
    },
};
use crate::{data, exporter::Exporter, receiver::Receiver};
use eyre::Result;
use metadata::LogstashMetadata;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Lookups,
    metadata: LogstashMetadata,
    #[serde(skip)]
    exporter: Exporter,
    #[serde(skip)]
    receiver: Receiver,
    #[serde(skip)]
    report: DiagnosticReport,
}

impl LogstashDiagnostic {
    async fn process<T>(&mut self) -> Result<()>
    where
        T: DataSource + DataProcessor<Lookups, LogstashMetadata> + DeserializeOwned + Send + Sync,
    {
        let data = self.receiver.get::<T>().await?;
        let (index, docs) = data.generate_docs(&self.lookups, &self.metadata);
        let summary = self.exporter.write(index, docs).await?;
        self.report.add_processor_summary(summary);
        Ok(())
    }
}

impl DiagnosticProcessor for LogstashDiagnostic {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
        let logstash_version = receiver.get::<version::Version>().await?;
        let metadata = LogstashMetadata::try_new(manifest, logstash_version)?;
        let plugins = receiver.get::<plugins::Plugins>().await?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .product(Product::Logstash)
            .receiver(receiver.to_string())
            .build()?;

        Ok(Box::new(Self {
            lookups: Lookups {
                plugin_count: plugins.total,
            },
            metadata,
            exporter,
            receiver,
            report,
        }))
    }

    async fn run(mut self) -> Result<DiagnosticReport> {
        log::debug!("Running Logstash diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        self.process::<node::Node>().await?;
        self.process::<node_stats::NodeStats>().await?;
        self.process::<plugins::Plugins>().await?;
        self.report.add_identifiers(self.exporter.identifiers());
        self.report.add_origin(
            Some(self.metadata.node.name.clone()),
            None,
            Some("node".to_string()),
        );

        self.exporter.save_report(&self.report).await?;
        Ok(self.report)
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }
}

#[derive(Serialize)]
struct Lookups {
    plugin_count: u32,
}
