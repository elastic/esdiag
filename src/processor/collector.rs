// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{elasticsearch::ElasticsearchCollector, logstash::LogstashCollector};
use crate::{
    data::Product,
    exporter::Exporter,
    processor::Identifiers,
    receiver::Receiver,
};
use eyre::{Result, eyre};

#[derive(Debug, Clone)]
pub struct CollectOptions {
    pub product: Product,
    pub r#type: String,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub identifiers: Identifiers,
}

pub enum Collector {
    Elasticsearch(ElasticsearchCollector),
    Logstash(LogstashCollector),
}

impl Collector {
    pub async fn try_new(
        receiver: Receiver,
        exporter: Exporter,
        product: Product,
        r#type: String,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        identifiers: Identifiers,
    ) -> Result<Self> {
        let options = CollectOptions {
            product,
            r#type,
            include,
            exclude,
            identifiers,
        };

        match (options.product.clone(), receiver) {
            (
                Product::Elasticsearch,
                receiver @ (Receiver::Elasticsearch(_) | Receiver::ElasticCloudAdmin(_)),
            ) => {
                let collect_exporter = exporter.into_collect_exporter()?;
                let collector =
                    ElasticsearchCollector::new(receiver, collect_exporter, options).await?;
                Ok(Self::Elasticsearch(collector))
            }
            (Product::Logstash, receiver @ Receiver::Elasticsearch(_)) => {
                let collect_exporter = exporter.into_collect_exporter()?;
                let collector = LogstashCollector::new(receiver, collect_exporter, options).await?;
                Ok(Self::Logstash(collector))
            }
            (Product::Logstash, _) => Err(eyre!(
                "Collect for Logstash requires a standard known-host endpoint"
            )),
            _ => Err(eyre!("Collect is only implemented for Elasticsearch and Logstash hosts")),
        }
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let result = match self {
            Self::Elasticsearch(collector) => collector.collect().await?,
            Self::Logstash(collector) => collector.collect().await?,
        };

        log::info!(
            "Collected {} of {} files into {}",
            result.success,
            result.total,
            result.path
        );
        Ok(result)
    }
}

pub struct CollectionResult {
    pub path: String,
    pub success: usize,
    pub total: usize,
}
