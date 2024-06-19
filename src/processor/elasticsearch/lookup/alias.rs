use super::{Lookup, LookupDisplay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    pub fn is_write_index(&self) -> bool {
        self.is_write_index.unwrap_or(false)
    }
}

impl From<String> for Lookup<AliasData> {
    fn from(string: String) -> Self {
        let index_alias: HashMap<String, IndexAlias> =
            serde_json::from_str(&string).expect("Failed to parse AliasData");
        let mut lookup_alias: Lookup<AliasData> = Lookup::new();

        for (name, data) in index_alias {
            for (alias, data) in data.aliases {
                let alias_data = AliasData::new(Some(alias), data.is_hidden, data.is_write_index);
                lookup_alias.add(alias_data).with_name(&name);
            }
        }
        log::debug!("lookup_alias entries: {}", lookup_alias.entries.len());
        lookup_alias
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IndexAlias {
    aliases: HashMap<String, AliasData>,
}

impl LookupDisplay for AliasData {
    fn display() -> &'static str {
        "alias_data"
    }
}
