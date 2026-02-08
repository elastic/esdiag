// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use tokio::sync::mpsc::Sender;

pub enum PathType {
    Url,
    File,
}

pub trait DataSource {
    fn source(path: PathType) -> Result<&'static str>;
    fn name() -> String;
}

pub trait StreamingDataSource: DataSource {
    type Item: Send + 'static;
    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>;
}

#[allow(dead_code)] // For future use deserialzing the sources.yml
#[derive(Clone, PartialEq, Serialize, Deserialize, Eq)]
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
        match &self.subdir {
            Some(subdir) => write!(fmt, "{}", subdir),
            None => Ok(()),
        }
    }
}
