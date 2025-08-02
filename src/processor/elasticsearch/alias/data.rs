use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

pub type AliasList = HashMap<String, Aliases>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Aliases {
    pub aliases: HashMap<String, AliasSettings>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AliasSettings {
    pub is_hidden: Option<bool>,
    pub is_write_index: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Alias {
    pub name: String,
    pub is_hidden: bool,
    pub is_write_index: bool,
}

impl Alias {
    pub fn with_name(self, name: String) -> Self {
        Self { name, ..self }
    }
}

impl From<AliasSettings> for Alias {
    fn from(data: AliasSettings) -> Self {
        Self {
            name: "".to_string(),
            is_hidden: data.is_hidden.unwrap_or(false),
            is_write_index: data.is_write_index.unwrap_or(false),
        }
    }
}

impl DataSource for AliasList {
    fn source(kind: PathType) -> Result<&'static str> {
        match kind {
            PathType::File => Ok("alias.json"),
            PathType::Url => Ok("_alias"),
        }
    }

    fn name() -> String {
        "alias".to_string()
    }
}
