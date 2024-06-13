use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
