pub mod elasticsearch;
use crate::input::{manifest::Manifest, DataSet};
use elasticsearch::metadata::Metadata;
use elasticsearch::EsDataSet;
use serde_json::Value;
use std::collections::HashMap;

pub struct Processor {
    pub metadata: Metadata,
}

impl Processor {
    pub fn new(manifest: &Manifest, metadata_content: HashMap<String, String>) -> Self {
        Processor {
            metadata: Metadata::new(manifest, metadata_content),
        }
    }
    pub fn enrich_lookup(&mut self, dataset: &DataSet, data: String) -> Option<Vec<Value>> {
        match dataset {
            DataSet::Elasticsearch(es_dataset) => match es_dataset {
                EsDataSet::Nodes => Some(elasticsearch::nodes::enrich_lookup(
                    &mut self.metadata,
                    data,
                )),
                EsDataSet::IndexSettings => Some(elasticsearch::index_settings::enrich_lookup(
                    &mut self.metadata,
                    data,
                )),
                EsDataSet::SearchableSnapshotStats => Some(
                    elasticsearch::searchable_snapshots_stats::enrich(&self.metadata, data),
                ),
                _ => None,
            },
            //DataSet::Kibana(kb_dataset) => match kb_dataset {
            //    _ => unimplemented!("Kibana"),
            //},
            //DataSet::Logstash(ls_dataset) => match ls_dataset {
            //    _ => unimplemented!("Logstash"),
            //},
        }
    }

    pub fn enrich(&self, dataset: &DataSet, data: Value) -> Vec<Value> {
        let empty = Vec::<Value>::new();
        match dataset {
            DataSet::Elasticsearch(es_dataset) => match es_dataset {
                EsDataSet::ClusterSettings => {
                    elasticsearch::cluster_settings::enrich(&self.metadata, data)
                }
                EsDataSet::IndexStats => elasticsearch::index_stats::enrich(&self.metadata, data),
                EsDataSet::NodesStats => elasticsearch::nodes_stats::enrich(&self.metadata, data),
                EsDataSet::Tasks => elasticsearch::tasks::enrich(&self.metadata, data),
                _ => empty,
            },
            //DataSet::Kibana(kb_dataset) => match kb_dataset {
            //    _ => unimplemented!("Kibana"),
            //},
            //DataSet::Logstash(ls_dataset) => match ls_dataset {
            //    _ => unimplemented!("Logstash"),
            //},
        }
    }
}
