/// Logstash diagnostic metadata
mod metadata;

use super::{DiagnosticProcessor, Metadata};
use crate::{
    data::{
        self,
        diagnostic::DiagnosticManifest,
        logstash::{
            LogstashHotThreads, LogstashNode, LogstashNodeStats, LogstashPlugins, LogstashVersion,
        },
    },
    exporter::Exporter,
    receiver::Receiver,
};
use color_eyre::eyre::Result;
use metadata::LogstashMetadata;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    metadata: Arc<LogstashMetadata>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl DiagnosticProcessor for LogstashDiagnostic {
    async fn new(
        manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
        let logstash_version = receiver.get::<LogstashVersion>().await?;
        let metadata = LogstashMetadata::try_new(manifest, logstash_version)?;

        Ok(Box::new(Self {
            metadata: Arc::new(metadata),
            exporter: Arc::new(exporter),
            receiver: Arc::new(receiver),
        }))
    }

    async fn run(self) -> Result<(String, usize)> {
        log::debug!("Running Logstash diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let output = self.receiver.get::<LogstashNode>().await?;
        log::debug!(
            "Logstash version: {}",
            serde_json::to_string(&output).unwrap()
        );
        self.receiver.get::<LogstashNodeStats>().await?;
        log::debug!(
            "Logstash version: {}",
            serde_json::to_string(&output).unwrap()
        );
        self.receiver.get::<LogstashPlugins>().await?;
        log::debug!(
            "Logstash version: {}",
            serde_json::to_string(&output).unwrap()
        );
        self.receiver.get::<LogstashHotThreads>().await?;
        log::debug!(
            "Logstash version: {}",
            serde_json::to_string(&output).unwrap()
        );
        Ok((String::from("Logstash"), 0))
    }

    async fn process_queue(&self) -> usize {
        0
    }
}
