// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::diagnostic::PathType;
use crate::processor::elasticsearch::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;

#[derive(Default, Deserialize, Serialize)]
pub struct MappingStats {
    #[serde(flatten)]
    pub indices: HashMap<String, IndexMapping>,
}

#[derive(Deserialize, Serialize)]
pub struct IndexMapping {
    pub mappings: Mappings,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Mappings {
    pub dynamic: Option<serde_json::Value>,
    pub date_detection: Option<bool>,
    pub numeric_detection: Option<bool>,
    pub dynamic_date_formats: Option<Vec<String>>,
    pub dynamic_templates: Option<Vec<serde_json::Value>>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _source: Option<SourceMode>,
    pub _meta: Option<serde_json::Value>,
    pub properties: Option<HashMap<String, FieldDefinition>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DataStreamTimestamp {
    pub enabled: bool,
}

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct SourceMode {
    pub mode: Option<String>,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct FieldDefinition {
    #[serde(rename = "type")]
    pub field_type: Option<String>,
    pub properties: Option<HashMap<String, FieldDefinition>>,
    pub fields: Option<HashMap<String, FieldDefinition>>,
}

#[skip_serializing_none]
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MappingSummary {
    pub dynamic: Option<serde_json::Value>,
    pub date_detection: Option<bool>,
    pub numeric_detection: Option<bool>,
    pub dynamic_date_formats: Option<Vec<String>>,
    pub dynamic_templates: Option<u32>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _source: Option<SourceMode>,
    pub _meta: Option<serde_json::Value>,
    pub field: FieldSummary,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct FieldSummary {
    pub fields: HashMap<String, u64>,
}

impl MappingStats {
    pub fn summaries(&self) -> HashMap<String, MappingSummary> {
        self.indices
            .iter()
            .map(|(name, index_mapping)| (name.clone(), index_mapping.summarize()))
            .collect()
    }
}

impl IndexMapping {
    pub fn summarize(&self) -> MappingSummary {
        let mut summary = MappingSummary {
            dynamic: self.mappings.dynamic.clone(),
            date_detection: self.mappings.date_detection,
            numeric_detection: self.mappings.numeric_detection,
            dynamic_date_formats: self.mappings.dynamic_date_formats.clone(),
            dynamic_templates: self
                .mappings
                .dynamic_templates
                .as_ref()
                .map(|t| t.len() as u32),
            _data_stream_timestamp: self.mappings._data_stream_timestamp.clone(),
            _source: self.mappings._source.clone(),
            _meta: self.mappings._meta.clone(),
            ..Default::default()
        };

        if let Some(properties) = &self.mappings.properties {
            for field in properties.values() {
                field.summarize(&mut summary.field);
            }
        }

        summary
    }
}

impl FieldDefinition {
    pub fn summarize(&self, summary: &mut FieldSummary) {
        *summary.fields.entry("total".to_string()).or_insert(0) += 1;
        if let Some(field_type) = &self.field_type {
            *summary.fields.entry(field_type.clone()).or_insert(0) += 1;
        } else if self.properties.is_some() {
            // It's likely an 'object' or 'nested' type without explicit 'type' field
            *summary.fields.entry("object".to_string()).or_insert(0) += 1;
        }

        if let Some(properties) = &self.properties {
            for field in properties.values() {
                field.summarize(summary);
            }
        }

        if let Some(fields) = &self.fields {
            for field in fields.values() {
                field.summarize(summary);
            }
        }
    }
}

impl DataSource for MappingStats {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("mapping.json"),
            PathType::Url => Ok("_mapping"),
        }
    }

    fn name() -> String {
        "mapping".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_mapping() {
        let json = r#"{
            "test_index": {
                "mappings": {
                    "dynamic": "strict",
                    "date_detection": false,
                    "numeric_detection": true,
                    "dynamic_date_formats": ["yyyy-MM-dd"],
                    "dynamic_templates": [{}, {}],
                    "_data_stream_timestamp": { "enabled": true },
                    "_source": { "mode": "synthetic" },
                    "properties": {
                        "field1": { "type": "text" },
                        "field2": { "type": "keyword" },
                        "object1": {
                            "properties": {
                                "subfield1": { "type": "long" }
                            }
                        }
                    }
                }
            }
        }"#;

        let stats: MappingStats = serde_json::from_str(json).unwrap();
        let summaries = stats.summaries();
        let summary = summaries.get("test_index").unwrap();

        assert_eq!(
            summary.dynamic,
            Some(serde_json::Value::String("strict".to_string()))
        );
        assert_eq!(summary.date_detection, Some(false));
        assert_eq!(summary.numeric_detection, Some(true));
        assert_eq!(
            summary.dynamic_date_formats,
            Some(vec!["yyyy-MM-dd".to_string()])
        );
        assert_eq!(summary.dynamic_templates, Some(2));
        assert!(summary._data_stream_timestamp.as_ref().unwrap().enabled);
        assert_eq!(
            summary._source.as_ref().unwrap().mode,
            Some("synthetic".to_string())
        );
        assert_eq!(summary.field.fields.get("total").unwrap(), &4); // field1, field2, object1, subfield1
        assert_eq!(summary.field.fields.get("text").unwrap(), &1);
        assert_eq!(summary.field.fields.get("keyword").unwrap(), &1);
        assert_eq!(summary.field.fields.get("long").unwrap(), &1);
        assert_eq!(summary.field.fields.get("object").unwrap(), &1);
    }

    #[test]
    fn test_summarize_multi_fields() {
        let json = r#"{
            "test_index": {
                "mappings": {
                    "properties": {
                        "field1": {
                            "type": "text",
                            "fields": {
                                "keyword": { "type": "keyword" }
                            }
                        }
                    }
                }
            }
        }"#;

        let stats: MappingStats = serde_json::from_str(json).unwrap();
        let summaries = stats.summaries();
        let summary = summaries.get("test_index").unwrap();

        assert_eq!(summary.field.fields.get("total").unwrap(), &2); // field1, field1.keyword
        assert_eq!(summary.field.fields.get("text").unwrap(), &1);
        assert_eq!(summary.field.fields.get("keyword").unwrap(), &1);
    }
}
