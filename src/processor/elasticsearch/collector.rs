// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{collector::CollectionResult, diagnostic::PathType};
use super::{
    AliasList, Cluster, ClusterSettings, DataSource, DataStreams, DiagnosticManifest, HealthReport,
    IlmExplain, IlmPolicies, IndicesSettings, IndicesStats, Licenses, Nodes, NodesStats,
    PendingTasks, SearchableSnapshotsCacheStats, SearchableSnapshotsStats, SlmPolicies, Tasks,
};
use crate::{data::Product, exporter::DirectoryExporter, receiver::Receiver};
use eyre::Result;
use std::path::PathBuf;

pub struct ElasticsearchCollector {
    receiver: Receiver,
    exporter: DirectoryExporter,
}

impl ElasticsearchCollector {
    pub async fn new(receiver: Receiver, exporter: DirectoryExporter) -> Result<Self> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let directory = format!("api-diagnostics-{}", timestamp);
        Ok(Self {
            receiver,
            exporter: exporter.collection_directory(directory)?,
        })
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let mut result = CollectionResult {
            path: self.exporter.to_string().clone(),
            success: 0,
            total: 18,
        };

        result.success += self.save_diagnostic_manifest().await?;
        result.success += self.save::<AliasList>().await?;
        result.success += self.save::<Cluster>().await?;
        result.success += self.save::<ClusterSettings>().await?;
        result.success += self.save::<DataStreams>().await?;
        result.success += self.save::<HealthReport>().await?;
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
