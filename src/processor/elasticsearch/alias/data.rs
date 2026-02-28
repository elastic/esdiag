// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

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
    fn source(path: PathType, version: Option<&semver::Version>) -> Result<String> {
        let name = Self::name();
        if let Ok(source_conf) =
            crate::processor::diagnostic::data_source::get_source(Self::product(), &name)
        {
            match path {
                PathType::File => Ok(source_conf.get_file_path(&name)),
                PathType::Url => {
                    let v = version.ok_or_else(|| eyre::eyre!("Version required for URL"))?;
                    source_conf.get_url(v)
                }
            }
        } else {
            // Fallback for missing or not-yet-supported sources
            eyre::bail!(
                "Source configuration missing for product: {}, name: {}",
                Self::product(),
                name
            )
        }
    }

    fn name() -> String {
        "alias".to_string()
    }
}
