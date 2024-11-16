use super::{DataProcessor, LogstashMetadata, Lookups};
use crate::{
    data::logstash::{Plugin, Plugins},
    processor::Metadata,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

impl DataProcessor<Lookups, LogstashMetadata> for Plugins {
    fn generate_docs(
        self,
        _: Arc<Lookups>,
        metadata: Arc<LogstashMetadata>,
    ) -> (String, Vec<Value>) {
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
