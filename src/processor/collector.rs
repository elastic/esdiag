// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::elasticsearch::ElasticsearchCollector;
use crate::{exporter::DirectoryExporter, processor::Identifiers, receiver::Receiver};
use eyre::{Result, eyre};

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub r#type: String,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub identifiers: Identifiers,
}

pub enum Collector {
    Elasticsearch(ElasticsearchCollector),
}

impl Collector {
    pub async fn try_new(
        receiver: Receiver,
        exporter: DirectoryExporter,
        r#type: String,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        identifiers: Identifiers,
    ) -> Result<Self> {
        let options = CollectOptions {
            r#type,
            include,
            exclude,
            identifiers,
        };

        if let Receiver::Elasticsearch(_) = &receiver {
            let collector = ElasticsearchCollector::new(receiver, exporter, options).await?;
            Ok(Self::Elasticsearch(collector))
        } else if let Receiver::ElasticCloudAdmin(_) = &receiver {
            let collector = ElasticsearchCollector::new(receiver, exporter, options).await?;
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
