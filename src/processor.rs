pub mod elasticsearch;
use elasticsearch::EsDataSet;
pub mod kibana;
pub mod logstash;
use crate::input::{manifest::Manifest, DataSet};
use elasticsearch::metadata::Metadata;
use serde_json::Value;
use std::collections::HashMap;

pub struct Processor {
    pub metadata: Metadata,
}

impl Processor {
    pub fn new(manifest: &Manifest, metadata: &HashMap<String, Value>) -> Self {
        Processor {
            metadata: Metadata::new(manifest, metadata),
        }
    }
    pub async fn enrich_lookup(&mut self, dataset: &DataSet, data: Value) -> Option<Vec<Value>> {
        match dataset {
            DataSet::Elasticsearch(es_dataset) => match es_dataset {
                EsDataSet::Alias => None,
                EsDataSet::DataStreams => None,
                EsDataSet::Nodes => {
                    Some(elasticsearch::nodes::enrich_lookup(&mut self.metadata, data).await)
                }
                EsDataSet::IndexSettings => Some(
                    elasticsearch::index_settings::enrich_lookup(&mut self.metadata, data).await,
                ),
                _ => None,
            },
            DataSet::Kibana(kb_dataset) => match kb_dataset {
                _ => unimplemented!("Kibana"),
            },
            DataSet::Logstash(ls_dataset) => match ls_dataset {
                _ => unimplemented!("Logstash"),
            },
        }
    }

    pub async fn enrich(&self, dataset: &DataSet, data: Value) -> Vec<Value> {
        let empty = Vec::<Value>::new();
        match dataset {
            DataSet::Elasticsearch(es_dataset) => match es_dataset {
                EsDataSet::ClusterSettings => {
                    elasticsearch::cluster_settings::enrich(&self.metadata, data).await
                }
                EsDataSet::IndexStats => {
                    elasticsearch::index_stats::enrich(&self.metadata, data).await
                }
                EsDataSet::NodesStats => {
                    elasticsearch::nodes_stats::enrich(&self.metadata, data).await
                }
                EsDataSet::Tasks => elasticsearch::tasks::enrich(&self.metadata, data).await,
                _ => empty,
            },
            DataSet::Kibana(kb_dataset) => match kb_dataset {
                _ => unimplemented!("Kibana"),
            },
            DataSet::Logstash(ls_dataset) => match ls_dataset {
                _ => unimplemented!("Logstash"),
            },
        }
    }
}
