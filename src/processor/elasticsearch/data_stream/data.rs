// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::DataSource;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

#[derive(Clone, Deserialize, Serialize)]
pub struct DataStreams {
    pub data_streams: Vec<DataStream>,
}

pub type Indices = Vec<IndexEntry>;

#[skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DataStream {
    pub allow_custom_routing: Option<bool>,
    pub generation: u64,
    pub hidden: Option<bool>,
    pub ilm_policy: Option<String>,
    #[serde(skip_serializing)]
    pub indices: Indices,
    pub name: String,
    pub next_generation_managed_by: Option<String>,
    pub prefer_ilm: Option<bool>,
    pub replicated: Option<bool>,
    pub rollover_on_write: Option<bool>,
    pub status: String,
    pub system: Option<bool>,
    pub template: Option<String>,
    pub timestamp_field: TimestampField,
    pub failure_store: Option<FailureStore>,
    #[serde(skip_deserializing)]
    pub dataset: String,
    #[serde(default)]
    pub is_write_index: bool,
    #[serde(skip_deserializing)]
    pub namespace: String,
    #[serde(skip_deserializing)]
    pub r#type: String,
}

impl DataStream {
    pub fn build(&mut self) {
        let words: Vec<&str> = self.name.split('-').collect();
        // Try to deconstruct the name into type, dataset, and namespace. If these
        // fields ever get added to the data stream API, we can avoid this mess
        match words.len() {
            1 => self.dataset = words[0].to_string(),
            2 if words[0] == ".fleet" => self.dataset = words[1..].join("-"),
            2 if words[0] == ".items" || words[0] == ".lists" => {
                self.namespace = words[1..].join("-");
            }
            3 => {
                self.r#type = words[0].to_string();
                self.dataset = words[1].to_string();
                self.namespace = words[2].to_string();
            }
            _ => {
                if words[0] == ".kibana" {
                    self.r#type = words[0].to_string();
                    match words[1] {
                        "reporting" => self.dataset = words[1].to_string(),
                        "elastic"
                            if words.len() > 3
                                && (words[4] == "anonymization"
                                    || words[4] == "knowledge"
                                    || words[4] == "attack") =>
                        {
                            self.dataset = words[1..6].join("-");
                            self.namespace = words[6..].join("-");
                        }
                        "elastic" => {
                            self.dataset = words[1..5].join("-");
                            self.namespace = words[5..].join("-");
                        }
                        "event" => {
                            self.dataset = words[1..3].join("-");
                            self.namespace = words[3..].join("-");
                        }
                        _ => {
                            self.dataset = words[1].to_string();
                            self.namespace = words[2..].join("-");
                        }
                    }
                } else {
                    self.r#type = words[0].to_string();
                    self.dataset = words[1].to_string();
                    self.namespace = words[2..].join("-");
                }
            }
        }
    }

    pub fn set_write_index(self, value: bool) -> Self {
        Self {
            is_write_index: value,
            ..self
        }
    }
}

#[skip_serializing_none]
#[derive(Clone, Deserialize, Serialize)]
pub struct DataStreamDocument {
    pub allow_custom_routing: Option<bool>,
    pub generation: u64,
    pub hidden: Option<bool>,
    pub ilm_policy: Option<String>,
    pub name: String,
    pub next_generation_managed_by: Option<String>,
    pub prefer_ilm: Option<bool>,
    pub replicated: Option<bool>,
    pub rollover_on_write: Option<bool>,
    pub status: String,
    pub system: Option<bool>,
    pub template: Option<String>,
    pub timestamp_field: TimestampField,
    pub failure_store: Option<FailureStore>,
    pub dataset: String,
    pub is_write_index: bool,
    pub namespace: String,
    pub r#type: String,
}

impl From<DataStream> for DataStreamDocument {
    fn from(data_stream: DataStream) -> Self {
        Self {
            allow_custom_routing: data_stream.allow_custom_routing,
            generation: data_stream.generation,
            hidden: data_stream.hidden,
            ilm_policy: data_stream.ilm_policy,
            name: data_stream.name,
            next_generation_managed_by: data_stream.next_generation_managed_by,
            prefer_ilm: data_stream.prefer_ilm,
            replicated: data_stream.replicated,
            rollover_on_write: data_stream.rollover_on_write,
            status: data_stream.status,
            system: data_stream.system,
            template: data_stream.template,
            timestamp_field: data_stream.timestamp_field,
            failure_store: data_stream.failure_store,
            dataset: data_stream.dataset,
            is_write_index: data_stream.is_write_index,
            namespace: data_stream.namespace,
            r#type: data_stream.r#type,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimestampField {
    pub name: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexEntry {
    pub index_name: String,
    pub index_uuid: String,
    pub prefer_ilm: Option<bool>,
    pub ilm_policy: Option<String>,
    pub managed_by: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FailureStore {
    pub enabled: bool,
    pub rollover_on_write: Option<bool>,
    pub indices: Vec<IndexEntry>,
    pub lifecycle: Option<FailureStoreLifecycle>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FailureStoreLifecycle {
    pub enabled: bool,
    pub effective_retention: Option<String>,
    pub retention_determined_by: Option<String>,
}

impl DataSource for DataStreams {

    fn name() -> String {
        "data_stream".to_string()
    }
}
