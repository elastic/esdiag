use crate::data::{
    diagnostic::{elasticsearch::DataSet, DataSource},
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
    pub template: String,
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
