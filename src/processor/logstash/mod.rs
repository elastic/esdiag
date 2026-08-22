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
    receiver::{MissingSource, Receiver},
};
use eyre::{Result, eyre};
use futures::future::BoxFuture;
use node::Node;
use node_stats::NodeStats;
use plugins::Plugins;
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc;

/// Runs one processable source's typed processor.
type LsProcessFn = for<'a> fn(&'a LogstashDiagnostic, mpsc::Sender<ProcessorSummary>) -> BoxFuture<'a, Result<()>>;

/// The registry-keyed dispatch table (ADR-0005): each entry binds one
/// processable source to its typed processor in a single registration — the
/// canonical registry key, the impl's `DataSource::name()`, and the call that
/// runs it. [`validate_ls_dispatch_registry`] proves the table and the
/// `sources.yml` registry agree.
struct LsDispatchEntry {
    /// Canonical registry key handled by this entry.
    key: &'static str,
    /// `DataSource::name()` of the typed impl bound to `key`.
    datasource_name: fn() -> String,
    /// Invokes that impl.
    process: LsProcessFn,
}

/// Binds a source's typed processor as an [`LsProcessFn`]. The type is named
/// concretely so the resulting future keeps its auto-derived `Send`.
macro_rules! processes {
    ($source:ty) => {{
        fn run<'a>(
            diagnostic: &'a LogstashDiagnostic,
            summary_tx: mpsc::Sender<ProcessorSummary>,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move { diagnostic.process_datasource::<$source>(summary_tx).await })
        }
        run
    }};
}

const LS_DISPATCH: &[LsDispatchEntry] = &[
    LsDispatchEntry {
        key: "logstash_node",
        datasource_name: datasource_name::<Node>,
        process: processes!(Node),
    },
    LsDispatchEntry {
        key: "logstash_node_stats",
        datasource_name: datasource_name::<NodeStats>,
        process: processes!(NodeStats),
    },
    LsDispatchEntry {
        key: "logstash_plugins",
        datasource_name: datasource_name::<Plugins>,
        process: processes!(Plugins),
    },
];

/// `DataSource::name()` as a plain function pointer, so a dispatch entry can
/// carry the name its key must equal.
fn datasource_name<T: DataSource>() -> String {
    T::name()
}

/// Fail fast if the dispatch keys and the collection registry disagree
/// (ADR-0005 key alignment). Runs once.
fn validate_ls_dispatch_registry() -> Result<()> {
    static VALIDATED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    VALIDATED
        .get_or_init(|| {
            let claims: Vec<ProcessableClaim> = LS_DISPATCH
                .iter()
                .map(|entry| ProcessableClaim {
                    key: entry.key,
                    datasource_name: (entry.datasource_name)(),
                })
                .collect();
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

    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource + DocumentExporter<Lookups, LogstashMetadata> + DeserializeOwned + Send + Sync,
    {
        let summary = match self.receiver.get::<T>().await {
            Ok(data) => {
                data.documents_export(&self.exporter, &self.lookups, &self.metadata)
                    .await
            }
            Err(err) if is_missing_source_error(&err) => {
                tracing::warn!("{}", err);
                ProcessorSummary::new(T::name())
            }
            Err(err) => return Err(err),
        };
        summary_tx.send(summary).await.map_err(|err| {
            tracing::error!("Failed to send summary: {}", err);
            eyre!(err)
        })
    }

    #[cfg(test)]
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
                (entry.process)(&self, summary_tx.clone()).await?;
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

fn is_missing_source_error(err: &eyre::Report) -> bool {
    err.chain().any(|cause| {
        cause.is::<MissingSource>()
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use super::{MissingSource, is_missing_source_error, validate_ls_dispatch_registry};
    use eyre::eyre;

    #[test]
    fn dispatch_table_and_registry_agree() {
        validate_ls_dispatch_registry().expect("Logstash dispatch table matches the collection registry");
    }

    #[test]
    fn missing_source_errors_are_tolerated() {
        let file_error = eyre!(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert!(is_missing_source_error(&file_error));

        let candidate_error: eyre::Report = MissingSource::NoCandidates {
            source: "logstash_plugins".to_string(),
        }
        .into();
        assert!(is_missing_source_error(&candidate_error));

        let archive_error: eyre::Report = MissingSource::ArchiveEntry {
            path: "logstash_plugins.json".to_string(),
        }
        .into();
        assert!(is_missing_source_error(&archive_error));
    }

    #[test]
    fn parse_errors_are_not_tolerated_as_missing_sources() {
        let parse_error = serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
        let parse_error = eyre!(parse_error);

        assert!(!is_missing_source_error(&parse_error));
    }

    #[test]
    fn messages_resembling_missing_sources_are_not_tolerated() {
        let lookalike = eyre!("File not found in archive: reported without the missing-source type");

        assert!(!is_missing_source_error(&lookalike));
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub plugin_count: u32,
}
