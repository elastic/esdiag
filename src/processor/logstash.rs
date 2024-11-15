use super::{lookup::Lookup, DiagnosticProcessor};
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
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Arc<Lookups>,
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
        let lookups = Arc::new(Lookups {});

        Ok(Box::new(Self {
            lookups,
            exporter: Arc::new(exporter),
            receiver: Arc::new(receiver),
        }))
    }

    async fn run(self) -> Result<(String, usize)> {
        log::debug!("Running Logstash diagnostic processors");
        if log::max_level() >= log::Level::Debug {
            data::save_file("diagnostic.json", &self)?;
        }

        let output = self.receiver.get::<LogstashVersion>().await?;
        log::debug!(
            "Logstash version: {}",
            serde_json::to_string(&output).unwrap()
        );
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

#[derive(Serialize)]
pub struct Lookups {}
