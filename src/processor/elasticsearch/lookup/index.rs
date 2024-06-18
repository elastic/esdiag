use super::LookupDisplay;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct IndexData {
    pub indexing_complete: Option<bool>,
    pub creation_date: Option<i64>,
}

impl IndexData {
    pub fn new() -> Self {
        IndexData {
            indexing_complete: None,
            creation_date: None,
        }
    }
}

impl std::default::Default for IndexData {
    fn default() -> Self {
        IndexData::new()
    }
}

impl LookupDisplay for IndexData {
    fn display() -> &'static str {
        "index_data"
    }
}
