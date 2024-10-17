use crate::data::Uri;
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub trait DataSource {
    fn source(uri: &Uri) -> Result<&'static str>;
    fn name() -> &'static str;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub struct Source {
    pub extension: Option<String>,
    pub subdir: Option<String>,
    pub versions: BTreeMap<String, String>,
}

impl Source {
    pub fn as_path_string(&self, name: &str) -> String {
        let mut string = String::new();
        match self.subdir {
            Some(ref subdir) => {
                string.push_str(subdir);
                string.push_str("/");
            }
            None => string.push_str(""),
        }
        string.push_str(name);
        match self.extension {
            Some(ref extension) => string.push_str(extension),
            None => string.push_str(".json"),
        }
        string
    }
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
