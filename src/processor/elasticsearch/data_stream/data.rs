use super::super::super::diagnostic::data_source::PathType;
use super::super::DataSource;
use eyre::Result;
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
    #[serde(skip_deserializing)]
    pub dataset: String,
    #[serde(skip_deserializing)]
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

impl DataSource for DataStreams {
    fn source(path: PathType) -> Result<&'static str> {
        match path {
            PathType::File => Ok("commercial/data_stream.json"),
            PathType::Url => Ok("_data_stream"),
        }
    }

    fn name() -> String {
        "data_stream".to_string()
    }
}
