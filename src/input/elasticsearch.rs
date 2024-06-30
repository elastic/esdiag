use super::{Application, DataSet};
use crate::processor::elasticsearch::EsDataSet::*;

/// Defines the data sets from an Elasticsearch diagnostic

pub struct Elasticsearch {
    pub data_sets: Vec<DataSet>,
    pub lookup_sets: Vec<DataSet>,
    pub metadata_sets: Vec<DataSet>,
}

impl Elasticsearch {
    pub fn new() -> Box<dyn Application> {
        let metadata_sets: Vec<DataSet> = Vec::from([
            DataSet::Elasticsearch(Alias),
            DataSet::Elasticsearch(Version),
            DataSet::Elasticsearch(DataStreams),
            DataSet::Elasticsearch(IlmExplain),
            DataSet::Elasticsearch(SharedCacheStats),
        ]);
        let lookup_sets: Vec<DataSet> = Vec::from([
            DataSet::Elasticsearch(Nodes),
            DataSet::Elasticsearch(IndexSettings),
            DataSet::Elasticsearch(SearchableSnapshotStats),
        ]);
        let data_sets: Vec<DataSet> = Vec::from([
            DataSet::Elasticsearch(ClusterSettings),
            DataSet::Elasticsearch(Tasks),
            DataSet::Elasticsearch(IndexStats),
            DataSet::Elasticsearch(NodesStats),
        ]);

        Box::new(Self {
            data_sets,
            lookup_sets,
            metadata_sets,
        })
    }
}

impl Application for Elasticsearch {
    fn get_metadata_sets(&self) -> Vec<DataSet> {
        self.metadata_sets.clone()
    }

    fn get_lookup_sets(&self) -> Vec<DataSet> {
        self.lookup_sets.clone()
    }

    fn get_data_sets(&self) -> Vec<DataSet> {
        self.data_sets.clone()
    }
}
