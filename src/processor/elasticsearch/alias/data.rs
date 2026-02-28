// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
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
    #[serde(default)]
    pub is_write_index: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
            is_write_index: data.is_write_index,
        }
    }
}

impl DataSource for AliasList {

    fn name() -> String {
        "alias".to_string()
    }
}
