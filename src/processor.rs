/// Processors for diagnostic bundles
pub mod diagnostic;
/// Processors for Elasticsearch diagnostics
pub mod elasticsearch;
/// Lookup processors
pub mod lookup;

use crate::data::diagnostic::{elasticsearch::EsDataSet, DataSet, Manifest};
use elasticsearch::metadata::Metadata;
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
                _ => None,
            },
        }
    }

    pub fn enrich(&self, dataset: &DataSet, data: String) -> Vec<Value> {
        match dataset {
            DataSet::Elasticsearch(es_dataset) => match es_dataset {
                EsDataSet::ClusterSettings => {
                    elasticsearch::cluster_settings::enrich(&self.metadata, data)
                }
                EsDataSet::IndexStats => elasticsearch::index_stats::enrich(&self.metadata, data),
                EsDataSet::NodesStats => elasticsearch::nodes_stats::enrich(&self.metadata, data),
                EsDataSet::Tasks => elasticsearch::tasks::enrich(&self.metadata, data),
                EsDataSet::SearchableSnapshotStats => {
                    elasticsearch::searchable_snapshots_stats::enrich(&self.metadata, data)
                }
                _ => Vec::<Value>::new(),
            },
        }
    }
}
