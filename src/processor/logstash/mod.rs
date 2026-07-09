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
    api::ProcessSelection,
    diagnostic::{
        DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder,
        data_source::{ProcessableClaim, validate_processable_registry},
    },
};
use crate::{
    data::{self, Application},
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use node::Node;
use node_stats::NodeStats;
use plugins::Plugins;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};
use tokio::sync::mpsc;

/// The registry-keyed dispatch table (ADR-0005): each entry binds one
/// processable source (by its canonical registry key) to its typed processor.
struct LsDispatchEntry {
    /// Canonical registry key handled by this entry.
    key: &'static str,
}

const LS_DISPATCH: &[LsDispatchEntry] = &[
    LsDispatchEntry { key: "logstash_node" },
    LsDispatchEntry {
        key: "logstash_node_stats",
    },
    LsDispatchEntry {
        key: "logstash_plugins",
    },
];

/// Fail fast if the dispatch keys and the collection registry disagree
/// (ADR-0005 key alignment). Runs once.
fn validate_ls_dispatch_registry() -> Result<()> {
    static VALIDATED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    VALIDATED
        .get_or_init(|| {
            let claims = vec![
                ProcessableClaim {
                    key: "logstash_node",
                    datasource_name: Node::name(),
                },
                ProcessableClaim {
                    key: "logstash_node_stats",
                    datasource_name: NodeStats::name(),
                },
                ProcessableClaim {
                    key: "logstash_plugins",
                    datasource_name: Plugins::name(),
                },
            ];
            let claim_keys = claims.iter().map(|claim| claim.key).collect::<BTreeSet<_>>();
            let dispatch_keys = LS_DISPATCH.iter().map(|entry| entry.key).collect::<BTreeSet<_>>();
            if claim_keys != dispatch_keys {
                return Err(format!(
                    "Logstash dispatch keys do not match processable claims: dispatch={dispatch_keys:?}, claims={claim_keys:?}"
                ));
            }
            validate_processable_registry("logstash", &claims).map_err(|err| err.to_string())
        })
        .clone()
        .map_err(|err| eyre!(err))
}

#[derive(Serialize)]
pub struct LogstashDiagnostic {
    lookups: Lookups,
    metadata: LogstashMetadata,
    selected_processors: Option<HashSet<String>>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl LogstashDiagnostic {
    fn should_process(&self, key: &str) -> bool {
        self.selected_processors
            .as_ref()
            .is_none_or(|selected| selected.contains(key))
    }

    async fn dispatch(&self, key: &'static str, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        match key {
            "logstash_node" => self.process_datasource::<Node>(summary_tx).await,
            "logstash_node_stats" => self.process_datasource::<NodeStats>(summary_tx).await,
            "logstash_plugins" => self.process_datasource::<Plugins>(summary_tx).await,
            other => Err(eyre!("No Logstash processor registered for '{other}'")),
        }
    }

    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource + DocumentExporter<Lookups, LogstashMetadata> + DeserializeOwned + Send + Sync,
    {
        let summary = match self.receiver.get::<T>().await {
            Ok(data) => {
                data.documents_export(&self.exporter, &self.lookups, &self.metadata)
                    .await
            }
            Err(err) => {
                tracing::warn!("{}", err);
                ProcessorSummary::new(T::name())
            }
        };
        summary_tx.send(summary).await.map_err(|err| {
            tracing::error!("Failed to send summary: {}", err);
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
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let logstash_version = receiver.get::<version::Version>().await?;
        let metadata = LogstashMetadata::try_new(manifest, logstash_version)?;
        let plugins = receiver.get::<plugins::Plugins>().await?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .application(Application::Logstash)
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
                selected_processors: process_selection.map(|selection| selection.selected.into_iter().collect()),
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        tracing::debug!("Running Logstash diagnostic processors");
        if tracing::enabled!(tracing::Level::DEBUG) {
            data::save_file("diagnostic.json", &self)?;
        }

        validate_ls_dispatch_registry()?;

        for entry in LS_DISPATCH {
            if self.should_process(entry.key) {
                self.dispatch(entry.key, summary_tx.clone()).await?;
            }
        }
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
