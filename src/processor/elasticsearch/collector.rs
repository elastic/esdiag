use super::super::{collector::CollectionResult, diagnostic::PathType};
use super::{
    DataSource, DiagnosticManifest, Product, alias::AliasList, cluster_settings::ClusterSettings,
    data_stream::DataStreams, ilm_explain::IlmExplain, ilm_policies::IlmPolicies,
    indices_settings::IndicesSettings, indices_stats::IndicesStats, licenses::Licenses,
    nodes::Nodes, nodes_stats::NodesStats, pending_tasks::PendingTasks,
    searchable_snapshots_cache_stats::SearchableSnapshotsCacheStats,
    searchable_snapshots_stats::SearchableSnapshotsStats, slm_policies::SlmPolicies, tasks::Tasks,
    version::Cluster,
};
use crate::{exporter::DirectoryExporter, receiver::Receiver};
use eyre::Result;
use std::path::PathBuf;

pub struct ElasticsearchCollector {
    receiver: Receiver,
    exporter: DirectoryExporter,
}

impl ElasticsearchCollector {
    pub async fn new(receiver: Receiver, exporter: DirectoryExporter) -> Result<Self> {
        Ok(Self { receiver, exporter })
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let mut result = CollectionResult {
            path: self.exporter.to_string().clone(),
            success: 0,
            total: 17,
        };

        result.success += self.save_diagnostic_manifest().await?;
        result.success += self.save::<AliasList>().await?;
        result.success += self.save::<Cluster>().await?;
        result.success += self.save::<ClusterSettings>().await?;
        result.success += self.save::<DataStreams>().await?;
        result.success += self.save::<IlmExplain>().await?;
        result.success += self.save::<IlmPolicies>().await?;
        result.success += self.save::<IndicesSettings>().await?;
        result.success += self.save::<IndicesStats>().await?;
        result.success += self.save::<Licenses>().await?;
        result.success += self.save::<Nodes>().await?;
        result.success += self.save::<NodesStats>().await?;
        result.success += self.save::<PendingTasks>().await?;
        result.success += self.save::<SearchableSnapshotsCacheStats>().await?;
        result.success += self.save::<SearchableSnapshotsStats>().await?;
        result.success += self.save::<SlmPolicies>().await?;
        result.success += self.save::<Tasks>().await?;

        Ok(result)
    }

    async fn save<T>(&self) -> Result<usize>
    where
        T: DataSource,
    {
        let content = self.receiver.get_raw::<T>().await?;
        let path = PathBuf::from(T::source(PathType::File)?);
        let filename = format!("{}", path.display());
        match self.exporter.save(path, content).await {
            Ok(()) => {
                log::info!("Saved {filename}");
                Ok(1)
            }
            Err(e) => {
                log::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save_diagnostic_manifest(&self) -> Result<usize> {
        let cluster = self.receiver.get::<Cluster>().await?;
        let manifest = DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some("standard".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(cluster.version.number.to_string()),
        );

        let path = PathBuf::from(DiagnosticManifest::source(PathType::File)?);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        log::info!("Saved {filename}");
        Ok(1)
    }
}
