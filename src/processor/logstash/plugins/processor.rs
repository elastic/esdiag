// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{DataProcessor, LogstashMetadata, Lookups, Metadata},
    Plugin, Plugins,
};
use serde::Serialize;
use serde_json::{Value, json};

impl DataProcessor<Lookups, LogstashMetadata> for Plugins {
    fn generate_docs(self, _: &Lookups, metadata: &LogstashMetadata) -> (String, Vec<Value>) {
        let data_stream = "settings-logstash.plugin-esdiag".to_string();
        let metadata_doc = metadata.for_data_stream(&data_stream).as_meta_doc();
        let docs: Vec<Value> = self
            .plugins
            .into_iter()
            .map(|plugin| json!(PluginDoc::new(plugin, metadata_doc.clone())))
            .collect();
        (data_stream, docs)
    }
}

#[derive(Serialize)]
struct PluginDoc {
    #[serde(flatten)]
    metadata: Value,
    plugin: Plugin,
}

impl PluginDoc {
    fn new(plugin: Plugin, metadata: Value) -> Self {
        Self { metadata, plugin }
    }
}
