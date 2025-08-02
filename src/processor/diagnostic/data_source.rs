use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub enum PathType {
    Url,
    File,
}

pub trait DataSource {
    fn source(path: PathType) -> Result<&'static str>;
    fn name() -> String;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub struct Source {
    pub extension: Option<String>,
    pub subdir: Option<String>,
    pub versions: BTreeMap<String, String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            extension: Some(String::from(".json")),
            subdir: None,
            versions: BTreeMap::new(),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{}", self)
    }
}
