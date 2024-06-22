use super::{Lookup, LookupDisplay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
pub struct AliasDoc {
    pub name: String,
    pub is_hidden: bool,
    pub is_write_index: bool,
}

impl AliasDoc {
    fn new(alias_name: String, data: AliasData) -> Self {
        Self {
            name: alias_name,
            is_hidden: data.is_hidden.unwrap_or(false),
            is_write_index: data.is_write_index.unwrap_or(false),
        }
    }
}

impl LookupDisplay for AliasDoc {
    fn display() -> &'static str {
        "alias_doc"
    }
}

impl From<String> for Lookup<AliasDoc> {
    fn from(string: String) -> Self {
        let index_alias: HashMap<String, IndexAlias> =
            serde_json::from_str(&string).expect("Failed to parse AliasData");
        let mut lookup_alias: Lookup<AliasDoc> = Lookup::new();

        for (index_name, index_data) in index_alias {
            for (alias_name, alias_data) in index_data.aliases {
                let alias_doc = AliasDoc::new(alias_name, alias_data);
                lookup_alias.add(alias_doc).with_name(&index_name);
            }
        }
        log::debug!("lookup_alias entries: {}", lookup_alias.entries.len());
        lookup_alias
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AliasData {
    is_hidden: Option<bool>,
    is_write_index: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct IndexAlias {
    aliases: HashMap<String, AliasData>,
}
