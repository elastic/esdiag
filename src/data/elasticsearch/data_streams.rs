use crate::data::{
    diagnostic::{data_source::DataSource, elasticsearch::DataSet},
    Uri,
};
use color_eyre::eyre::{eyre, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct DataStreams {
    pub data_streams: Vec<DataStream>,
}

pub type Indices = Vec<Index>;

#[derive(Clone, Deserialize, Serialize)]
pub struct DataStream {
    pub allow_custom_routing: Option<bool>,
    pub generation: u64,
    pub hidden: Option<bool>,
    pub ilm_policy: Option<String>,
    #[serde(skip_serializing)]
    pub indices: Indices,
    #[serde(skip_deserializing)]
    pub is_write_index: bool,
    pub name: String,
    pub next_generation_managed_by: Option<String>,
    pub prefer_ilm: Option<bool>,
    pub replicated: Option<bool>,
    pub rollover_on_write: Option<bool>,
    pub status: String,
    pub system: Option<bool>,
    pub template: Option<String>,
    pub timestamp_field: TimestampField,
}

impl DataStream {
    pub fn set_write_index(self, value: bool) -> Self {
        Self {
            is_write_index: value,
            ..self
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TimestampField {
    pub name: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Index {
    pub index_name: String,
    pub index_uuid: String,
    pub prefer_ilm: Option<bool>,
    pub ilm_policy: Option<String>,
    pub managed_by: Option<String>,
}

impl DataSource for DataStreams {
    fn source(uri: &Uri) -> Result<&'static str> {
        match uri {
            Uri::Directory(_) | Uri::File(_) => Ok("commercial/data_stream.json"),
            Uri::Host(_) | Uri::Url(_) => Ok("_data_stream"),
            _ => Err(eyre!("Unsuppored source for data_stream")),
        }
    }

    fn name() -> String {
        format!("{}", DataSet::DataStreams)
    }
}

#[derive(Clone, Serialize)]
pub struct DataStreamName {
    dataset: String,
    namespace: String,
    r#type: String,
}

impl From<&str> for DataStreamName {
    fn from(name: &str) -> Self {
        let terms: Vec<&str> = name.split('-').collect();
        DataStreamName {
            r#type: terms[0].to_string(),
            dataset: terms[1].to_string(),
            namespace: terms[2].to_string(),
        }
    }
}

impl ToString for DataStreamName {
    fn to_string(&self) -> String {
        format!("{}-{}-{}", self.r#type, self.dataset, self.namespace)
    }
}
