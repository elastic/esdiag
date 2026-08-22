// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// The `_alias` API
mod alias;
/// The `_cluster/settings` API
mod cluster_settings;
/// Collector definition for Elasticsearch diagnostics
mod collector;
/// The `_data_stream` API
mod data_stream;
/// The `_health_report` API
mod health_report;
/// The `_ilm/explain` API
mod ilm_explain;
/// The `_ilm/policy` API
mod ilm_policies;
/// The `_settings` API
mod indices_settings;
/// The `_stats` API
mod indices_stats;
/// The `_license` API
mod licenses;
/// The `_mapping` API
mod mapping_stats;
/// Elasticsearch diagnostics metadata
mod metadata;
/// The `_nodes` API
mod nodes;
/// The `_nodes/stats` API
mod nodes_stats;
/// The `_pending_tasks` API
mod pending_tasks;
/// The `_searchable_snapshots_cache/stats` API
mod searchable_snapshots_cache_stats;
/// The `_searchable_snapshots/stats` API
mod searchable_snapshots_stats;
/// The `_slm/policy` API
mod slm_policies;
/// The `_snapshot` API
mod snapshots;
/// The `_tasks` API
mod tasks;
/// The cluster `/` API -- "You know, for search!"
mod version;

use crate::processor::{StreamingDataSource, StreamingDocumentExporter};
pub use collector::ElasticsearchCollector;
pub use metadata::ElasticsearchMetadata;
use tokio::sync::mpsc;
pub use {
    licenses::{License, Licenses},
    version::{Cluster, ClusterMetadata, Version},
};

use super::{
    DataSource, DiagnosticManifest, DiagnosticProcessor, DiagnosticReport, DocumentExporter, Metadata,
    ProcessorSummary,
    api::{ProcessSelection, ProcessingConcurrencyPolicy, is_streamable, processing_weight},
    diagnostic::{
        DiagnosticReportBuilder, Lookup,
        data_source::{ProcessableClaim, validate_processable_registry},
    },
    elasticsearch::health_report::HealthReport,
};
use crate::{
    data::{self, Application},
    exporter::Exporter,
    receiver::{MissingSource, Receiver},
};
use eyre::{Result, eyre};
use futures::{future::BoxFuture, stream::FuturesUnordered};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashSet, sync::Arc};
use {
    alias::{Alias, AliasList},
    cluster_settings::{ClusterSettings, ClusterSettingsDefaults},
    data_stream::{DataStreamDocument, DataStreams},
    ilm_explain::{IlmExplain, IlmStats},
    ilm_policies::IlmPolicies,
    indices_settings::{IndexSettings, IndicesSettings},
    indices_stats::IndicesStats,
    mapping_stats::{MappingStats, MappingSummary},
    nodes::{NodeDocument, Nodes},
    nodes_stats::NodesStats,
    pending_tasks::PendingTasks,
    searchable_snapshots_cache_stats::{SearchableSnapshotsCacheStats, SharedCacheStats},
    searchable_snapshots_stats::SearchableSnapshotsStats,
    slm_policies::SlmPolicies,
    snapshots::{Repositories, Snapshots},
    tasks::Tasks,
};

#[derive(Serialize)]
pub struct ElasticsearchDiagnostic {
    lookups: Lookups,
    metadata: ElasticsearchMetadata,
    selected_processors: Option<HashSet<String>>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

/// Runs one processable source's typed processor.
type EsProcessFn = fn(Arc<ElasticsearchDiagnostic>, mpsc::Sender<ProcessorSummary>) -> BoxFuture<'static, Result<()>>;

/// The registry-keyed dispatch table (ADR-0005): each entry binds one
/// processable source to its typed processor in a single registration — the
/// canonical registry key, the impl's `DataSource::name()`, and the call that
/// runs it. Adding a processable source is this entry plus its `sources.yml`
/// entry; [`validate_es_dispatch_registry`] proves the two agree.
struct EsDispatchEntry {
    /// Canonical registry key handled by this entry.
    key: &'static str,
    /// `DataSource::name()` of the typed impl bound to `key`.
    datasource_name: fn() -> String,
    /// Invokes that impl.
    process: EsProcessFn,
}

/// `DataSource::name()` as a plain function pointer, so a dispatch entry can
/// carry the name its key must equal.
fn datasource_name<T: DataSource>() -> String {
    T::name()
}

/// Binds a buffered source's typed processor as an [`EsProcessFn`]. The type
/// is named concretely so the resulting future keeps its auto-derived `Send`,
/// which the web server's handlers require.
macro_rules! buffered {
    ($source:ty) => {{
        fn run(
            diagnostic: Arc<ElasticsearchDiagnostic>,
            summary_tx: mpsc::Sender<ProcessorSummary>,
        ) -> BoxFuture<'static, Result<()>> {
            Box::pin(async move { diagnostic.process_datasource::<$source>(summary_tx).await })
        }
        run
    }};
}

/// As [`buffered!`], for a source whose `streamable` registry flag selects
/// between the streaming and buffered paths.
macro_rules! streamable {
    ($source:ty) => {{
        fn run(
            diagnostic: Arc<ElasticsearchDiagnostic>,
            summary_tx: mpsc::Sender<ProcessorSummary>,
        ) -> BoxFuture<'static, Result<()>> {
            Box::pin(async move { diagnostic.process_maybe_streaming::<$source>(summary_tx).await })
        }
        run
    }};
}

/// Both cluster-settings keys export the same dataset through one processor.
fn process_cluster_settings(
    diagnostic: Arc<ElasticsearchDiagnostic>,
    summary_tx: mpsc::Sender<ProcessorSummary>,
) -> BoxFuture<'static, Result<()>> {
    Box::pin(async move { diagnostic.process_cluster_settings(summary_tx).await })
}

const ES_DISPATCH: &[EsDispatchEntry] = &[
    EsDispatchEntry {
        key: "indices_stats",
        datasource_name: datasource_name::<IndicesStats>,
        process: streamable!(IndicesStats),
    },
    EsDispatchEntry {
        key: "nodes_stats",
        datasource_name: datasource_name::<NodesStats>,
        process: streamable!(NodesStats),
    },
    EsDispatchEntry {
        key: "cluster_settings",
        datasource_name: datasource_name::<ClusterSettings>,
        process: process_cluster_settings,
    },
    EsDispatchEntry {
        key: "cluster_settings_defaults",
        datasource_name: datasource_name::<ClusterSettingsDefaults>,
        process: process_cluster_settings,
    },
    EsDispatchEntry {
        key: "health_report",
        datasource_name: datasource_name::<HealthReport>,
        process: buffered!(HealthReport),
    },
    EsDispatchEntry {
        key: "ilm_policies",
        datasource_name: datasource_name::<IlmPolicies>,
        process: buffered!(IlmPolicies),
    },
    EsDispatchEntry {
        key: "indices_settings",
        datasource_name: datasource_name::<IndicesSettings>,
        process: buffered!(IndicesSettings),
    },
    EsDispatchEntry {
        key: "nodes",
        datasource_name: datasource_name::<Nodes>,
        process: buffered!(Nodes),
    },
    EsDispatchEntry {
        key: "cluster_pending_tasks",
        datasource_name: datasource_name::<PendingTasks>,
        process: buffered!(PendingTasks),
    },
    EsDispatchEntry {
        key: "slm_policies",
        datasource_name: datasource_name::<SlmPolicies>,
        process: buffered!(SlmPolicies),
    },
    EsDispatchEntry {
        key: "repositories",
        datasource_name: datasource_name::<Repositories>,
        process: buffered!(Repositories),
    },
    EsDispatchEntry {
        key: "searchable_snapshots_stats",
        datasource_name: datasource_name::<SearchableSnapshotsStats>,
        process: buffered!(SearchableSnapshotsStats),
    },
    EsDispatchEntry {
        key: "snapshot",
        datasource_name: datasource_name::<Snapshots>,
        process: streamable!(Snapshots),
    },
    EsDispatchEntry {
        key: "tasks",
        datasource_name: datasource_name::<Tasks>,
        process: buffered!(Tasks),
    },
];

/// Fail fast if the dispatch table and the collection registry disagree
/// (ADR-0005 key alignment): every table key must be a registry entry marked
/// `processable`, matching its impl's `DataSource::name()`, and every
/// `processable` registry entry must appear in the table. Runs once.
fn validate_es_dispatch_registry() -> Result<()> {
    static VALIDATED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    VALIDATED
        .get_or_init(|| {
            let claims: Vec<ProcessableClaim> = ES_DISPATCH
                .iter()
                .map(|entry| ProcessableClaim {
                    key: entry.key,
                    datasource_name: (entry.datasource_name)(),
                })
                .collect();
            validate_processable_registry("elasticsearch", &claims).map_err(|err| err.to_string())
        })
        .clone()
        .map_err(|err| eyre!(err))
}

impl ElasticsearchDiagnostic {
    fn should_process(&self, key: &str) -> bool {
        self.selected_processors
            .as_ref()
            .is_none_or(|selected| selected.contains(key))
    }

    async fn process_maybe_streaming<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource
            + StreamingDataSource
            + StreamingDocumentExporter<Lookups, ElasticsearchMetadata>
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
        T::Item: DeserializeOwned + Send + 'static,
    {
        if is_streamable("elasticsearch", &T::name()) {
            self.process_streaming_datasource::<T>(summary_tx).await
        } else {
            self.process_datasource::<T>(summary_tx).await
        }
    }

    async fn process_cluster_settings(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        let summary = match self.receiver.get::<ClusterSettingsDefaults>().await {
            Ok(settings) => settings
                .documents_export(&self.exporter, &self.lookups, &self.metadata)
                .await
                .was_parsed(),
            Err(defaults_err) => {
                tracing::debug!(
                    "Failed to read cluster_settings_defaults, falling back to cluster_settings: {}",
                    defaults_err
                );
                match self.receiver.get::<ClusterSettings>().await {
                    Ok(settings) => settings
                        .documents_export(&self.exporter, &self.lookups, &self.metadata)
                        .await
                        .was_parsed(),
                    Err(settings_err) => {
                        if missing_source_error(&defaults_err) && missing_source_error(&settings_err) {
                            tracing::debug!(
                                "cluster_settings_defaults and cluster_settings are absent: {}; {}",
                                defaults_err,
                                settings_err
                            );
                            ProcessorSummary::missing(ClusterSettings::name())
                        } else {
                            tracing::warn!(
                                "Failed to read cluster_settings_defaults and cluster_settings: {}; {}",
                                defaults_err,
                                settings_err
                            );
                            ProcessorSummary::new(ClusterSettings::name()).with_error(format!(
                                "Failed to read cluster_settings_defaults and cluster_settings: {}; {}",
                                defaults_err, settings_err
                            ))
                        }
                    }
                }
            }
        };

        summary_tx.send(summary).await.map_err(|err| {
            tracing::error!("Failed to send summary: {}", err);
            eyre!(err)
        })
    }

    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource + DocumentExporter<Lookups, ElasticsearchMetadata> + DeserializeOwned + Send + Sync,
    {
        match self.receiver.get::<T>().await {
            Ok(data) => {
                let summary = data
                    .documents_export(&self.exporter, &self.lookups, &self.metadata)
                    .await
                    .was_parsed();
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
            Err(err) => {
                let summary = if missing_source_error(&err) {
                    tracing::debug!("{} is absent: {}", T::name(), err);
                    ProcessorSummary::missing(T::name())
                } else {
                    tracing::warn!(
                        "Failed to process data source {} (aliases: {}): {}",
                        T::name(),
                        T::aliases().join(", "),
                        err
                    );
                    ProcessorSummary::new(T::name()).with_error(err.to_string())
                };
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
        }
    }

    async fn process_streaming_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource
            + StreamingDataSource
            + StreamingDocumentExporter<Lookups, ElasticsearchMetadata>
            + DocumentExporter<Lookups, ElasticsearchMetadata>
            + DeserializeOwned
            + Send
            + Sync,
        T::Item: DeserializeOwned + Send + 'static,
    {
        match self.receiver.get_stream::<T>().await {
            Ok(stream) => {
                let summary = T::documents_export_stream(stream, &self.exporter, &self.lookups, &self.metadata)
                    .await
                    .was_parsed();
                summary_tx.send(summary).await.map_err(|err| {
                    tracing::error!("Failed to send summary: {}", err);
                    eyre!(err)
                })
            }
            Err(e) => {
                tracing::debug!(
                    "Streaming failed/not supported for {}, falling back to full load: {}",
                    T::name(),
                    e
                );
                self.process_datasource::<T>(summary_tx).await
            }
        }
    }
}

pub(super) fn missing_source_error(err: &eyre::Report) -> bool {
    err.chain().any(|cause| {
        cause.is::<MissingSource>()
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io_err| io_err.kind() == std::io::ErrorKind::NotFound)
    })
}

fn lookup_from_result<T, U>(result: Result<U>, label: &str) -> Lookup<T>
where
    T: Clone + Serialize,
    Lookup<T>: From<U>,
{
    match result {
        Ok(value) => Lookup::<T>::from_parsed(value),
        Err(err) if missing_source_error(&err) => Lookup::missing(),
        Err(err) => {
            tracing::warn!("Failed to parse {}: {}", label, err);
            Lookup::new()
        }
    }
}

impl DiagnosticProcessor for ElasticsearchDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        tracing::debug!("ElasticsearchDiagnostic::try_new start");
        let cluster = receiver.get::<version::Cluster>().await?;
        tracing::debug!("ElasticsearchDiagnostic::try_new loaded cluster");
        let display_name = match receiver.get::<ClusterSettingsDefaults>().await {
            Ok(settings) => settings.get_display_name(),
            Err(err) => {
                tracing::debug!(
                    "Failed to read cluster_settings_defaults for display name, falling back to cluster_settings: {}",
                    err
                );
                receiver.get::<ClusterSettings>().await?.get_display_name()
            }
        };
        tracing::debug!("ElasticsearchDiagnostic::try_new resolved display name");
        let metadata = ElasticsearchMetadata::try_new(manifest, cluster.with_display_name(display_name))?;
        tracing::debug!("ElasticsearchDiagnostic::try_new built metadata");

        let mut report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .cluster(metadata.cluster.clone())
            .application(Application::Elasticsearch)
            .receiver(receiver.to_string())
            .build()?;
        tracing::debug!("ElasticsearchDiagnostic::try_new built report");

        let lookups = Lookups {
            alias: lookup_from_result(receiver.get::<AliasList>().await, "AliasList"),
            data_stream: lookup_from_result(receiver.get::<DataStreams>().await, "DataStreams"),
            index_settings: lookup_from_result(receiver.get::<IndicesSettings>().await, "IndicesSettings"),
            node: lookup_from_result(receiver.get::<Nodes>().await, "Nodes"),
            ilm_explain: lookup_from_result(receiver.get::<IlmExplain>().await, "IlmExplain"),
            shared_cache: lookup_from_result(
                receiver.get::<SearchableSnapshotsCacheStats>().await,
                "SearchableSnapshotsCacheStats",
            ),
            mapping_stats: match receiver.get_stream::<MappingStats>().await {
                Ok(stream) => Lookup::<MappingSummary>::from_stream(stream).await,
                Err(e) => {
                    tracing::debug!("Streaming mappings failed: {}, falling back to full load", e);
                    lookup_from_result(receiver.get::<MappingStats>().await, "MappingStats")
                }
            },
        };
        tracing::debug!("ElasticsearchDiagnostic::try_new built lookups");
        let license = receiver.get::<Licenses>().await.map(|licenses| licenses.license).ok();

        report.add_license(license);
        report.add_lookup("alias", &lookups.alias);
        report.add_lookup("data_stream", &lookups.data_stream);
        report.add_lookup("index_settings", &lookups.index_settings);
        report.add_lookup("node", &lookups.node);
        report.add_lookup("ilm_explain", &lookups.ilm_explain);
        report.add_lookup("shared_cache", &lookups.shared_cache);
        report.add_lookup("mapping_stats", &lookups.mapping_stats);

        Ok((
            Box::new(Self {
                exporter,
                lookups,
                metadata,
                receiver,
                selected_processors: process_selection.map(|selection| selection.selected.into_iter().collect()),
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        tracing::debug!("Running Elasticsearch diagnostic processors");
        if !self.exporter.is_connected().await {
            return Err(eyre!("Exporter is not connected"));
        }

        if tracing::enabled!(tracing::Level::DEBUG) {
            data::save_file("diagnostic.json", &self)?;
        }

        validate_es_dispatch_registry()?;

        let diag = Arc::new(self);
        // Processing weight governs processing concurrency (ADR-0017): the
        // heaviest sources run as their own concurrent tasks; the rest run
        // sequentially. The weight -> concurrency mapping is tunable policy
        // (ADR-0018).
        let policy = ProcessingConcurrencyPolicy::from_env();
        let mut concurrent = FuturesUnordered::new();
        let mut sequential = Vec::new();
        let process_cluster_settings_defaults = diag.should_process("cluster_settings_defaults");
        for entry in ES_DISPATCH {
            if !diag.should_process(entry.key) {
                continue;
            }
            // Both keys export the same cluster-settings dataset. When both
            // are selected, run the shared defaults-first processor once.
            if entry.key == "cluster_settings" && process_cluster_settings_defaults {
                continue;
            }
            let weight = processing_weight("elasticsearch", entry.key);
            if policy.is_concurrent(weight) {
                concurrent.push((entry.process)(diag.clone(), summary_tx.clone()));
            } else {
                sequential.push(entry);
            }
        }

        let sequential_task = async {
            for entry in sequential {
                (entry.process)(diag.clone(), summary_tx.clone()).await?;
            }
            Ok::<(), eyre::Error>(())
        };
        let concurrent_task = async {
            while let Some(result) = futures::StreamExt::next(&mut concurrent).await {
                result?;
            }
            Ok::<(), eyre::Error>(())
        };

        let _ = tokio::try_join!(sequential_task, concurrent_task)?;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            self.metadata.cluster.display_name.clone(),
            self.metadata.cluster.uuid.clone(),
            "cluster".to_string(),
        )
    }
}

impl ElasticsearchDiagnostic {
    #[cfg(test)]
    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }
}

#[derive(Clone, Serialize)]
pub struct Lookups {
    pub alias: Lookup<Alias>,
    pub data_stream: Lookup<DataStreamDocument>,
    pub ilm_explain: Lookup<IlmStats>,
    pub index_settings: Lookup<IndexSettings>,
    pub mapping_stats: Lookup<MappingSummary>,
    pub node: Lookup<NodeDocument>,
    pub shared_cache: Lookup<SharedCacheStats>,
}

#[cfg(test)]
mod tests {
    use super::{missing_source_error, validate_es_dispatch_registry};
    use crate::receiver::MissingSource;

    #[test]
    fn dispatch_table_and_registry_agree() {
        validate_es_dispatch_registry().expect("Elasticsearch dispatch table matches the collection registry");
    }

    #[test]
    fn empty_source_files_are_treated_as_missing() {
        let error = eyre::Report::new(MissingSource::Empty {
            path: "internal_health.json".to_string(),
        });

        assert!(missing_source_error(&error));
    }
}
