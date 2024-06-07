use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct IndexData {
    age: Option<u64>,
}

impl IndexData {
    pub fn new() -> IndexData {
        IndexData { age: None }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IndexLookup {
    pub by_index: HashMap<String, IndexData>,
}

impl IndexLookup {
    pub fn new() -> IndexLookup {
        IndexLookup {
            by_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, index: &str, data: &Value) {
        self.by_index.insert(
            index.to_string(),
            IndexData {
                age: data["age"].as_u64(),
            },
        );
    }
}
