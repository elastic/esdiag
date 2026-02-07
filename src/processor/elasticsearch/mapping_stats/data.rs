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
    pub dynamic: Option<String>,
    pub date_detection: Option<bool>,
    pub numeric_detection: Option<bool>,
    pub dynamic_date_formats: Option<Vec<String>>,
    pub dynamic_templates: Option<u32>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _source: Option<SourceMode>,
    pub _meta: Option<serde_json::Value>,
    pub fields: HashMap<String, u64>,
    #[serde(rename = "multi-fields")]
    pub multi_fields: MultiFieldSummary,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MultiFieldSummary {
    pub total: u64,
    pub names: Vec<String>,
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
            dynamic: self.mappings.dynamic.as_ref().map(|v| match v {
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => v.to_string(),
            }),
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
            for (name, field) in properties {
                field.summarize(name, &mut summary.fields, &mut summary.multi_fields);
            }
        }

        summary
    }
}

impl FieldDefinition {
    pub fn summarize(
        &self,
        name: &str,
        fields: &mut HashMap<String, u64>,
        multi_fields: &mut MultiFieldSummary,
    ) {
        // Increment total count, avoiding repeated String allocation
        if let Some(count) = fields.get_mut("total") {
            *count += 1;
        } else {
            fields.insert("total".to_string(), 1);
        }

        if let Some(field_type) = &self.field_type {
            // Increment count for this field type, cloning only on first insert
            if let Some(count) = fields.get_mut(field_type) {
                *count += 1;
            } else {
                fields.insert(field_type.clone(), 1);
            }
        } else if self.properties.is_some() {
            // It's likely an 'object' or 'nested' type without explicit 'type' field
            if let Some(count) = fields.get_mut("object") {
                *count += 1;
            } else {
                fields.insert("object".to_string(), 1);
            }
        }

        // Check if it's a multi-field mapping (has fields property)
        if let Some(fields_map) = &self.fields {
            if !fields_map.is_empty() {
                multi_fields.total += 1;
                multi_fields.names.push(name.to_string());
            }
        }

        if let Some(properties) = &self.properties {
            for (sub_name, field) in properties {
                let mut full_name = String::with_capacity(name.len() + 1 + sub_name.len());
                full_name.push_str(name);
                full_name.push('.');
                full_name.push_str(sub_name);
                field.summarize(&full_name, fields, multi_fields);
            }
        }

        if let Some(fields_map) = &self.fields {
            for (sub_name, field) in fields_map {
                let mut full_name = String::with_capacity(name.len() + 1 + sub_name.len());
                full_name.push_str(name);
                full_name.push('.');
                full_name.push_str(sub_name);
                field.summarize(&full_name, fields, multi_fields);
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

        assert_eq!(summary.dynamic, Some("strict".to_string()));
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
        assert_eq!(summary.fields.get("total").unwrap(), &4); // field1, field2, object1, subfield1
        assert_eq!(summary.fields.get("text").unwrap(), &1);
        assert_eq!(summary.fields.get("keyword").unwrap(), &1);
        assert_eq!(summary.fields.get("long").unwrap(), &1);
        assert_eq!(summary.fields.get("object").unwrap(), &1);
        assert_eq!(summary.multi_fields.total, 0);
        assert!(summary.multi_fields.names.is_empty());
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

        assert_eq!(summary.fields.get("total").unwrap(), &2); // field1, field1.keyword
        assert_eq!(summary.fields.get("text").unwrap(), &1);
        assert_eq!(summary.fields.get("keyword").unwrap(), &1);
        assert_eq!(summary.multi_fields.total, 1); // field1 is a multi-field
        assert_eq!(summary.multi_fields.names, vec!["field1".to_string()]);
    }

    #[test]
    fn test_summarize_edge_cases() {
        let json = r#"{
            "test_index": {
                "mappings": {
                    "properties": {
                        "empty_fields": {
                            "type": "text",
                            "fields": {}
                        },
                        "no_type_with_fields": {
                            "fields": {
                                "keyword": { "type": "keyword" }
                            }
                        },
                        "object_with_no_type": {
                            "properties": {
                                "sub": { "type": "keyword" }
                            }
                        }
                    }
                }
            }
        }"#;

        let stats: MappingStats = serde_json::from_str(json).unwrap();
        let summaries = stats.summaries();
        let summary = summaries.get("test_index").unwrap();

        // empty_fields should NOT count as multi-field because map is empty
        // no_type_with_fields SHOULD count as multi-field even without explicit 'type' (Elasticsearch default)
        // object_with_no_type should count as 'object' in fields map
        assert_eq!(summary.multi_fields.total, 1);
        assert_eq!(
            summary.multi_fields.names,
            vec!["no_type_with_fields".to_string()]
        );
        assert_eq!(summary.fields.get("object").unwrap(), &1); // object_with_no_type
    }
}
