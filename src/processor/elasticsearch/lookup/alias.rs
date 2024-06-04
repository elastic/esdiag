use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AliasData {
    alias: Option<String>,
    is_hidden: Option<bool>,
    is_write_index: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AliasLookup {
    pub by_index: HashMap<String, Option<AliasData>>,
}

impl AliasLookup {
    pub fn new() -> AliasLookup {
        AliasLookup {
            by_index: HashMap::new(),
        }
    }

    pub fn from_value(aliases: Value) -> AliasLookup {
        let mut alias_lookup: AliasLookup = AliasLookup::new();

        for (index, data) in aliases.as_object().cloned().expect("aliases not an object") {
            //println!("{:?}, {:?}", index, data);
            if let Some(aliases) = data["aliases"].as_object() {
                for (name, props) in aliases {
                    let alias_data = Some(AliasData {
                        alias: Some(String::from(name)),
                        is_write_index: props["is_write_index"].as_bool(),
                        is_hidden: props["is_hidden"].as_bool(),
                    });
                    alias_lookup
                        .by_index
                        .insert(index.clone(), alias_data.clone());
                }
            }
        }
        alias_lookup
    }
}
