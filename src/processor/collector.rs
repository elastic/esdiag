// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{elasticsearch::ElasticsearchCollector, kibana::KibanaCollector, logstash::LogstashCollector};
use crate::{
    data::Product,
    exporter::Exporter,
    processor::{Identifiers, RequestedApi},
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
    Kibana(KibanaCollector),
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
            (Product::Elasticsearch, receiver @ (Receiver::Elasticsearch(_) | Receiver::ElasticCloudAdmin(_))) => {
                let collect_exporter = exporter.into_collect_exporter()?;
                let collector = ElasticsearchCollector::new(receiver, collect_exporter, options).await?;
                Ok(Self::Elasticsearch(collector))
            }
            (Product::Logstash, receiver @ Receiver::Logstash(_)) => {
                let collect_exporter = exporter.into_collect_exporter()?;
                let collector = LogstashCollector::new(receiver, collect_exporter, options).await?;
                Ok(Self::Logstash(collector))
            }
            (Product::Kibana, receiver @ Receiver::Kibana(_)) => {
                let collect_exporter = exporter.into_collect_exporter()?;
                let collector = KibanaCollector::new(receiver, collect_exporter, options).await?;
                Ok(Self::Kibana(collector))
            }
            (Product::Logstash, _) => Err(eyre!("Collect for Logstash requires a standard known-host endpoint")),
            (Product::Kibana, _) => Err(eyre!("Collect for Kibana requires a standard known-host endpoint")),
            _ => Err(eyre!(
                "Collect is only implemented for Elasticsearch, Kibana, and Logstash hosts"
            )),
        }
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let result = match self {
            Self::Elasticsearch(collector) => collector.collect().await?,
            Self::Logstash(collector) => collector.collect().await?,
            Self::Kibana(collector) => collector.collect().await?,
        };

        tracing::info!(
            "Collected {} of {} files into {}",
            result.success,
            result.total,
            result.path
        );
        Ok(result)
    }
}

pub fn default_collect_archive_name(product: &Product, timestamp: &str) -> String {
    match product {
        Product::Elasticsearch => format!("api-diagnostics-{timestamp}"),
        Product::Kibana => format!("kibana-api-diagnostics-{timestamp}"),
        Product::Logstash => format!("logstash-api-diagnostics-{timestamp}"),
        Product::Agent => format!("agent-api-diagnostics-{timestamp}"),
        Product::ECE => format!("ece-api-diagnostics-{timestamp}"),
        Product::ECK => format!("eck-api-diagnostics-{timestamp}"),
        Product::ElasticCloudHosted => {
            format!("elastic-cloud-hosted-api-diagnostics-{timestamp}")
        }
        Product::KubernetesPlatform => {
            format!("kubernetes-platform-api-diagnostics-{timestamp}")
        }
        Product::Unknown => format!("unknown-api-diagnostics-{timestamp}"),
    }
}

pub struct CollectionResult {
    pub path: String,
    pub success: usize,
    pub total: usize,
}

pub(crate) struct ApiCollectOutcome {
    pub(crate) requested_api: Option<(String, RequestedApi)>,
    pub(crate) saved: usize,
}

impl ApiCollectOutcome {
    pub(crate) fn skipped() -> Self {
        Self {
            requested_api: None,
            saved: 0,
        }
    }

    pub(crate) fn success(name: &str, mut requested_api: RequestedApi, retries: u32, saved: usize) -> Self {
        requested_api.retries = retries;
        Self {
            requested_api: Some((name.to_string(), requested_api)),
            saved,
        }
    }

    pub(crate) fn failed(
        name: &str,
        status: Option<u16>,
        retries: u32,
        response_time_ms: u64,
        response_size_bytes: u64,
    ) -> Self {
        Self {
            requested_api: Some((
                name.to_string(),
                RequestedApi {
                    status,
                    retries,
                    response_time_ms,
                    response_size_bytes,
                },
            )),
            saved: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::default_collect_archive_name;
    use crate::data::Product;

    #[test]
    fn default_archive_name_uses_elasticsearch_basename_without_prefix() {
        assert_eq!(
            default_collect_archive_name(&Product::Elasticsearch, "20260406-203000"),
            "api-diagnostics-20260406-203000"
        );
    }

    #[test]
    fn default_archive_name_prefixes_non_elasticsearch_products() {
        assert_eq!(
            default_collect_archive_name(&Product::Logstash, "20260406-203000"),
            "logstash-api-diagnostics-20260406-203000"
        );
        assert_eq!(
            default_collect_archive_name(&Product::Kibana, "20260406-203000"),
            "kibana-api-diagnostics-20260406-203000"
        );
    }
}
