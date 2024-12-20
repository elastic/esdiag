use crate::{
    data::{
        diagnostic::{data_source::PathType, DataSource, DiagnosticManifest, Product},
        elasticsearch::{
            AliasList, Cluster, ClusterSettings, DataStreams, IlmExplain, IndicesSettings,
            IndicesStats, Nodes, NodesStats, SearchableSnapshotsCacheStats,
            SearchableSnapshotsStats, Tasks,
        },
    },
    exporter::DirectoryExporter,
    receiver::Receiver,
};
use color_eyre::eyre::{eyre, Result};
use std::path::PathBuf;

pub enum Collector {
    Elasticsearch(ElasticsearchCollector),
}

impl Collector {
    pub async fn try_new(receiver: Receiver, exporter: DirectoryExporter) -> Result<Self> {
        if let Receiver::Elasticsearch(_) = &receiver {
            let collector = ElasticsearchCollector::new(receiver, exporter).await?;
            Ok(Self::Elasticsearch(collector))
        } else {
            Err(eyre!(
                "Collect is only implemented from Elasticsearch to a Directory"
            ))
        }
    }

    pub async fn collect(&self) -> Result<usize> {
        match self {
            Self::Elasticsearch(collector) => collector.collect().await,
        }
    }
}

pub struct ElasticsearchCollector {
    receiver: Receiver,
    exporter: DirectoryExporter,
}

impl ElasticsearchCollector {
    pub async fn new(receiver: Receiver, exporter: DirectoryExporter) -> Result<Self> {
        Ok(Self { receiver, exporter })
    }

    pub async fn collect(&self) -> Result<usize> {
        let mut file_count = 0;
        let total = 13;

        file_count += self.save_diagnostic_manifest().await?;
        file_count += self.save::<AliasList>().await?;
        file_count += self.save::<Cluster>().await?;
        file_count += self.save::<ClusterSettings>().await?;
        file_count += self.save::<DataStreams>().await?;
        file_count += self.save::<IlmExplain>().await?;
        file_count += self.save::<IndicesSettings>().await?;
        file_count += self.save::<IndicesStats>().await?;
        file_count += self.save::<Nodes>().await?;
        file_count += self.save::<NodesStats>().await?;
        file_count += self.save::<SearchableSnapshotsCacheStats>().await?;
        file_count += self.save::<SearchableSnapshotsStats>().await?;
        file_count += self.save::<Tasks>().await?;

        log::info!(
            "Collected {file_count} of {total} files into {}",
            self.exporter
        );
        Ok(file_count)
    }

    async fn save<T>(&self) -> Result<usize>
    where
        T: DataSource,
    {
        let content = self.receiver.get_raw::<T>().await?;
        let path = PathBuf::from(T::source(PathType::File)?);
        let filename = format!("{}", path.display());
        match self.exporter.save(path, content).await {
            Ok(()) => Ok(1),
            Err(e) => {
                log::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save_diagnostic_manifest(&self) -> Result<usize> {
        let cluster = self.receiver.get::<Cluster>().await?;
        let manifest = DiagnosticManifest {
            name: Some(cluster.diagnostic_node.clone()),
            collection_date: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            diagnostic: Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            flags: None,
            included_diagnostics: None,
            mode: Some("esdiag".to_string()),
            product: Product::Elasticsearch,
            r#type: Some("elasticsearch_diagnostic".to_string()),
            runner: Some("esdiag".to_string()),
            version: Some(cluster.version.number.to_string()),
        };
        let path = PathBuf::from(DiagnosticManifest::source(PathType::File)?);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        log::debug!("Saved {filename}.json");
        Ok(1)
    }
}
