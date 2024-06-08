use super::{Identifiers, Lookup};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IndexAlias {
    aliases: HashMap<String, AliasData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AliasData {
    alias: Option<String>,
    is_hidden: Option<bool>,
    is_write_index: Option<bool>,
}

impl AliasData {
    pub fn new(
        alias: Option<String>,
        is_hidden: Option<bool>,
        is_write_index: Option<bool>,
    ) -> Self {
        Self {
            alias,
            is_hidden,
            is_write_index,
        }
    }
}

impl From<String> for Lookup<AliasData> {
    fn from(string: String) -> Self {
        let index_alias: HashMap<String, IndexAlias> =
            serde_json::from_str(&string).expect("Failed to parse AliasData");
        let mut lookup_alias: Lookup<AliasData> = Lookup::new();

        for (index, data) in index_alias {
            for (alias, data) in data.aliases {
                let ids = Identifiers {
                    id: None,
                    name: Some(index.clone()),
                    host: None,
                    ip: None,
                };
                lookup_alias.insert(
                    ids,
                    AliasData::new(Some(alias), data.is_hidden, data.is_write_index),
                );
            }
        }
        lookup_alias
    }
}
