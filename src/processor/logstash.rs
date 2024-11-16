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
        diagnostic::{DataSource, DiagnosticManifest},
        logstash::{Node, NodeStats, Plugins, Version},
    },
    exporter::Exporter,
    receiver::Receiver,
};
use color_eyre::eyre::Result;
use metadata::LogstashMetadata;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Arc<Lookups>,
    metadata: Arc<LogstashMetadata>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl LogstashDiagnostic {
    async fn process<T>(&self) -> Result<usize>
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

        Ok(Box::new(Self {
            lookups: Arc::new(Lookups {
                plugin_count: plugins.total,
            }),
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

        let mut doc_count = 0;
        doc_count += self.process::<Node>().await?;
        doc_count += self.process::<NodeStats>().await?;
        doc_count += self.process::<Plugins>().await?;

        Ok((String::from("Logstash"), doc_count))
    }

    async fn process_queue(&self) -> usize {
        0
    }
}

#[derive(Serialize)]
struct Lookups {
    plugin_count: u32,
}
