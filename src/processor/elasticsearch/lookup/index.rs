use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct IndexData {
    pub indexing_complete: Option<bool>,
    pub creation_date: Option<i64>,
}
