use super::elasticsearch::collector::ElasticsearchCollector;
use crate::{exporter::DirectoryExporter, receiver::Receiver};
use eyre::{Result, eyre};

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

    pub async fn collect(&self) -> Result<()> {
        let result = match self {
            Self::Elasticsearch(collector) => collector.collect().await?,
        };

        log::info!(
            "Collected {} of {} files into {}",
            result.success,
            result.total,
            result.path
        );
        Ok(())
    }
}

pub struct CollectionResult {
    pub path: String,
    pub success: usize,
    pub total: usize,
}
