use super::{Application, DataSet};

pub struct Logstash {
    data_sets: Vec<DataSet>,
    lookup_sets: Vec<DataSet>,
    metadata_sets: Vec<DataSet>,
}

impl Logstash {
    pub fn new() -> Box<dyn Application> {
        Box::new(Self {
            data_sets: Vec::new(),
            lookup_sets: Vec::new(),
            metadata_sets: Vec::new(),
        })
    }
}

impl Application for Logstash {
    fn get_data_sets(&self) -> Vec<DataSet> {
        self.data_sets.clone()
    }

    fn get_lookup_sets(&self) -> Vec<DataSet> {
        self.lookup_sets.clone()
    }

    fn get_metadata_sets(&self) -> Vec<DataSet> {
        self.metadata_sets.clone()
    }
}
