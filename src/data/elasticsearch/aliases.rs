use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type AliasList = HashMap<String, Aliases>;

#[derive(Clone, Debug, Deserialize)]
pub struct Aliases {
    pub aliases: HashMap<String, AliasSettings>,
}
#[derive(Clone, Debug, Deserialize)]
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
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("alias.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_alias"),
            _ => Err(eyre!("Unsuppored source for alias")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::Aliases)
    }
}
