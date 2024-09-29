use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct IndexData {
    pub age: Option<i64>,
    pub codec: String,
    pub creation_date: Option<i64>,
    pub hidden: Option<String>,
    pub indexing_complete: Option<bool>,
    pub refresh_interval: String,
}

impl IndexData {
    pub fn new() -> Self {
        IndexData {
            age: None,
            codec: String::new(),
            creation_date: None,
            hidden: None,
            indexing_complete: None,
            refresh_interval: String::new(),
        }
    }
}

impl Default for IndexData {
    fn default() -> Self {
        IndexData::new()
    }
}

impl AsRef<IndexData> for IndexData {
    fn as_ref(&self) -> &IndexData {
        self
    }
}
