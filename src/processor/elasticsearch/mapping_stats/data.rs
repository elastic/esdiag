// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::processor::diagnostic::data_source::StreamingDataSource;
use crate::processor::elasticsearch::DataSource;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

#[derive(Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MappingStats {
    pub indices: HashMap<String, IndexMapping>,
}

impl MappingStats {
    pub fn summaries(&self) -> HashMap<String, MappingSummary> {
        self.indices
            .iter()
            .map(|(name, index_mapping)| (name.clone(), index_mapping.summarize()))
            .collect()
    }
}

impl StreamingDataSource for MappingStats {
    type Item = (String, IndexMapping);

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MappingStatsVisitor {
            sender: Sender<Result<(String, IndexMapping)>>,
        }

        impl<'de> serde::de::Visitor<'de> for MappingStatsVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("MappingStats object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<IndexMapping>()?;
                    if self.sender.blocking_send(Ok((key, value))).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            }
        }

        deserializer.deserialize_map(MappingStatsVisitor { sender })
    }
}

#[derive(Deserialize, Serialize)]
pub struct IndexMapping {
    pub mappings: Mappings,
}

#[skip_serializing_none]
#[derive(Deserialize, Serialize)]
pub struct Mappings {
    pub dynamic: Option<Box<serde_json::value::RawValue>>,
    pub date_detection: Option<bool>,
    pub numeric_detection: Option<bool>,
    pub dynamic_date_formats: Option<Vec<String>>,
    #[serde(default, deserialize_with = "count_sequence_elements")]
    pub dynamic_templates: Option<u32>,
    pub _data_stream_timestamp: Option<DataStreamTimestamp>,
    pub _source: Option<SourceMode>,
    pub _meta: Option<Box<serde_json::value::RawValue>>,
    pub properties: Option<HashMap<String, FieldDefinition>>,
}

fn count_sequence_elements<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct SequenceCounter;

    impl<'de> serde::de::Visitor<'de> for SequenceCounter {
        type Value = Option<u32>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of objects")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut count = 0;
            while let Some(serde::de::IgnoredAny) = seq.next_element()? {
                count += 1;
            }
            Ok(Some(count))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }
    }

    deserializer.deserialize_option(SequenceCounter)
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
    pub _meta: Option<Box<serde_json::value::RawValue>>,
    pub fields: HashMap<String, u64>,
    #[serde(rename = "multi-fields")]
    pub multi_fields: MultiFieldSummary,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MultiFieldSummary {
    pub total: u64,
    pub names: Vec<String>,
}

impl IndexMapping {
    pub fn summarize(&self) -> MappingSummary {
        let mut summary =
            MappingSummary {
                dynamic: self.mappings.dynamic.as_ref().map(|v| {
                    match serde_json::from_str::<serde_json::Value>(v.get()) {
                        Ok(serde_json::Value::String(s)) => s,
                        Ok(val) => val.to_string(),
                        Err(_) => v.get().to_string(),
                    }
                }),
                date_detection: self.mappings.date_detection,
                numeric_detection: self.mappings.numeric_detection,
                dynamic_date_formats: self.mappings.dynamic_date_formats.clone(),
                dynamic_templates: self.mappings.dynamic_templates,
                _data_stream_timestamp: self.mappings._data_stream_timestamp.clone(),
                _source: self.mappings._source.clone(),
                _meta: self.mappings._meta.clone(),
                ..Default::default()
            };

        if let Some(properties) = &self.mappings.properties {
            let mut path = String::with_capacity(128);
            for (name, field) in properties {
                path.push_str(name);
                field.summarize(&mut path, &mut summary.fields, &mut summary.multi_fields);
                path.clear();
            }
        }

        summary
    }
}

impl FieldDefinition {
    pub fn summarize(
        &self,
        path: &mut String,
        fields: &mut HashMap<String, u64>,
        multi_fields: &mut MultiFieldSummary,
    ) {
        // Increment total count
        if let Some(count) = fields.get_mut("total") {
            *count += 1;
        } else {
            fields.insert("total".to_string(), 1);
        }

        if let Some(field_type) = &self.field_type {
            if let Some(count) = fields.get_mut(field_type) {
                *count += 1;
            } else {
                fields.insert(field_type.clone(), 1);
            }
        } else if self.properties.is_some() {
            if let Some(count) = fields.get_mut("object") {
                *count += 1;
            } else {
                fields.insert("object".to_string(), 1);
            }
        }

        // Check if it's a multi-field mapping (has fields property)
        if self.fields.as_ref().is_some_and(|f| !f.is_empty()) {
            multi_fields.total += 1;
            multi_fields.names.push(path.clone());
        }

        if let Some(properties) = &self.properties {
            let original_len = path.len();
            path.push('.');
            let dot_len = path.len();

            for (sub_name, field) in properties {
                path.push_str(sub_name);
                field.summarize(path, fields, multi_fields);
                path.truncate(dot_len);
            }
            path.truncate(original_len);
        }

        if let Some(fields_map) = &self.fields {
            let original_len = path.len();
            path.push('.');
            let dot_len = path.len();

            for (sub_name, field) in fields_map {
                path.push_str(sub_name);
                field.summarize(path, fields, multi_fields);
                path.truncate(dot_len);
            }
            path.truncate(original_len);
        }
    }
}

impl DataSource for MappingStats {
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
        assert_eq!(summary.dynamic_date_formats, Some(vec!["yyyy-MM-dd".to_string()]));
        assert_eq!(summary.dynamic_templates, Some(2));
        assert!(summary._data_stream_timestamp.as_ref().unwrap().enabled);
        assert_eq!(summary._source.as_ref().unwrap().mode, Some("synthetic".to_string()));
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
        assert_eq!(summary.multi_fields.names, vec!["no_type_with_fields".to_string()]);
        assert_eq!(summary.fields.get("object").unwrap(), &1); // object_with_no_type
    }

    #[tokio::test]
    async fn test_streaming_deserialization() {
        use crate::processor::diagnostic::data_source::StreamingDataSource;
        use crate::processor::elasticsearch::mapping_stats::MappingStats;
        use tokio::sync::mpsc;

        let json = r#"{
            "index1": {
                "mappings": {
                    "properties": {
                        "field1": { "type": "text" }
                    }
                }
            },
            "index2": {
                "mappings": {
                    "properties": {
                        "field1": { "type": "keyword" }
                    }
                }
            }
        }"#;

        let mut deserializer = serde_json::Deserializer::from_str(json);
        let (tx, mut rx) = mpsc::channel(10);

        let handle = tokio::task::spawn_blocking(move || {
            MappingStats::deserialize_stream(&mut deserializer, tx).unwrap();
        });

        let mut count = 0;
        while let Some(res) = rx.recv().await {
            assert!(res.is_ok());
            count += 1;
        }
        assert_eq!(count, 2);
        handle.await.unwrap();
    }
}
