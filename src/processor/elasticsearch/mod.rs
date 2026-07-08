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
    licenses::License,
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
    receiver::Receiver,
};
use eyre::{Result, eyre};
use futures::stream::FuturesUnordered;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};
use {
    alias::{Alias, AliasList},
    cluster_settings::{ClusterSettings, ClusterSettingsDefaults},
    data_stream::{DataStreamDocument, DataStreams},
    ilm_explain::{IlmExplain, IlmStats},
    ilm_policies::IlmPolicies,
    indices_settings::{IndexSettings, IndicesSettings},
    indices_stats::IndicesStats,
    licenses::Licenses,
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

/// The registry-keyed dispatch table (ADR-0005): each entry binds one
/// processable source (by its canonical registry key) to its typed processor.
/// Validated against the registry at process time by [`validate_es_dispatch_registry`].
struct EsDispatchEntry {
    /// Canonical registry key handled by this entry.
    key: &'static str,
}

const ES_DISPATCH: &[EsDispatchEntry] = &[
    EsDispatchEntry { key: "indices_stats" },
    EsDispatchEntry { key: "nodes_stats" },
    EsDispatchEntry {
        key: "cluster_settings",
    },
    EsDispatchEntry {
        key: "cluster_settings_defaults",
    },
    EsDispatchEntry { key: "health_report" },
    EsDispatchEntry { key: "ilm_policies" },
    EsDispatchEntry {
        key: "indices_settings",
    },
    EsDispatchEntry { key: "nodes" },
    EsDispatchEntry {
        key: "cluster_pending_tasks",
    },
    EsDispatchEntry { key: "slm_policies" },
    EsDispatchEntry { key: "repositories" },
    EsDispatchEntry {
        key: "searchable_snapshots_stats",
    },
    EsDispatchEntry { key: "snapshot" },
    EsDispatchEntry { key: "tasks" },
];

/// Fail fast if the dispatch table and the collection registry disagree
/// (ADR-0005 key alignment): every table key must be a registry entry marked
/// `processable`, matching its impl's `DataSource::name()`, and every
/// `processable` registry entry must appear in the table. Runs once.
fn validate_es_dispatch_registry() -> Result<()> {
    static VALIDATED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    VALIDATED
        .get_or_init(|| {
            let claims = vec![
                ProcessableClaim {
                    key: "indices_stats",
                    datasource_name: IndicesStats::name(),
                },
                ProcessableClaim {
                    key: "nodes_stats",
                    datasource_name: NodesStats::name(),
                },
                ProcessableClaim {
                    key: "cluster_settings",
                    datasource_name: ClusterSettings::name(),
                },
                ProcessableClaim {
                    key: "cluster_settings_defaults",
                    datasource_name: ClusterSettingsDefaults::name(),
                },
                ProcessableClaim {
                    key: "health_report",
                    datasource_name: HealthReport::name(),
                },
                ProcessableClaim {
                    key: "ilm_policies",
                    datasource_name: IlmPolicies::name(),
                },
                ProcessableClaim {
                    key: "indices_settings",
                    datasource_name: IndicesSettings::name(),
                },
                ProcessableClaim {
                    key: "nodes",
                    datasource_name: Nodes::name(),
                },
                ProcessableClaim {
                    key: "cluster_pending_tasks",
                    datasource_name: PendingTasks::name(),
                },
                ProcessableClaim {
                    key: "slm_policies",
                    datasource_name: SlmPolicies::name(),
                },
                ProcessableClaim {
                    key: "repositories",
                    datasource_name: Repositories::name(),
                },
                ProcessableClaim {
                    key: "searchable_snapshots_stats",
                    datasource_name: SearchableSnapshotsStats::name(),
                },
                ProcessableClaim {
                    key: "snapshot",
                    datasource_name: Snapshots::name(),
                },
                ProcessableClaim {
                    key: "tasks",
                    datasource_name: Tasks::name(),
                },
            ];
            let claim_keys = claims.iter().map(|claim| claim.key).collect::<BTreeSet<_>>();
            let dispatch_keys = ES_DISPATCH.iter().map(|entry| entry.key).collect::<BTreeSet<_>>();
            if claim_keys != dispatch_keys {
                return Err(format!(
                    "Elasticsearch dispatch keys do not match processable claims: dispatch={dispatch_keys:?}, claims={claim_keys:?}"
                ));
            }
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

    /// Route one canonical registry key to its typed processor. The
    /// `streamable` registry flag gates the streaming path (ADR-0005).
    async fn dispatch(&self, key: &'static str, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        match key {
            "indices_stats" => self.process_maybe_streaming::<IndicesStats>(summary_tx).await,
            "nodes_stats" => self.process_maybe_streaming::<NodesStats>(summary_tx).await,
            "cluster_settings" => self.process_datasource::<ClusterSettings>(summary_tx).await,
            "cluster_settings_defaults" => self.process_datasource::<ClusterSettingsDefaults>(summary_tx).await,
            "health_report" => self.process_datasource::<HealthReport>(summary_tx).await,
            "ilm_policies" => self.process_datasource::<IlmPolicies>(summary_tx).await,
            "indices_settings" => self.process_datasource::<IndicesSettings>(summary_tx).await,
            "nodes" => self.process_datasource::<Nodes>(summary_tx).await,
            "cluster_pending_tasks" => self.process_datasource::<PendingTasks>(summary_tx).await,
            "slm_policies" => self.process_datasource::<SlmPolicies>(summary_tx).await,
            "repositories" => self.process_datasource::<Repositories>(summary_tx).await,
            "searchable_snapshots_stats" => self.process_datasource::<SearchableSnapshotsStats>(summary_tx).await,
            "snapshot" => self.process_maybe_streaming::<Snapshots>(summary_tx).await,
            "tasks" => self.process_datasource::<Tasks>(summary_tx).await,
            other => Err(eyre!("No Elasticsearch processor registered for '{other}'")),
        }
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
                tracing::warn!("{}", err);
                let summary = ProcessorSummary::new(T::name());
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
            alias: Lookup::from(receiver.get::<AliasList>().await),
            data_stream: Lookup::from(receiver.get::<DataStreams>().await),
            index_settings: Lookup::from(receiver.get::<IndicesSettings>().await),
            node: Lookup::from(receiver.get::<Nodes>().await),
            ilm_explain: Lookup::from(receiver.get::<IlmExplain>().await),
            shared_cache: Lookup::from(receiver.get::<SearchableSnapshotsCacheStats>().await),
            mapping_stats: match receiver.get_stream::<MappingStats>().await {
                Ok(stream) => Lookup::<MappingSummary>::from_stream(stream).await,
                Err(e) => {
                    tracing::debug!("Streaming mappings failed: {}, falling back to full load", e);
                    Lookup::from(receiver.get::<MappingStats>().await)
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
        for entry in ES_DISPATCH {
            if !diag.should_process(entry.key) {
                continue;
            }
            let weight = processing_weight("elasticsearch", entry.key);
            if policy.is_concurrent(weight) {
                let (diag, tx) = (diag.clone(), summary_tx.clone());
                concurrent.push(async move { diag.dispatch(entry.key, tx).await });
            } else {
                sequential.push(entry.key);
            }
        }

        let sequential_task = async {
            for key in sequential {
                diag.dispatch(key, summary_tx.clone()).await?;
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
