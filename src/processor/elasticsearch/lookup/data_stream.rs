use super::{Identifiers, Lookup};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DataStreamData {
    name: String,
    timestamp_field: TimestampField,
    indices: Vec<Index>,
    generation: u64,
    status: String,
    template: String,
    ilm_policy: Option<String>,
    next_generation_managed_by: Option<String>,
    prefer_ilm: Option<bool>,
    hidden: Option<bool>,
    system: Option<bool>,
    allow_custom_routing: Option<bool>,
    replicated: Option<bool>,
    rollover_on_write: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TimestampField {
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DataStreamWrapper {
    data_streams: Vec<DataStreamData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Index {
    index_name: String,
    index_uuid: String,
    prefer_ilm: Option<bool>,
    ilm_policy: Option<String>,
    managed_by: Option<String>,
}

impl From<&String> for Lookup<DataStreamData> {
    fn from(string: &String) -> Self {
        let data_streams: DataStreamWrapper =
            serde_json::from_str(&string).expect("Failed to parse DataStreamData");

        let mut lookup_data_stream: Lookup<DataStreamData> = Lookup::new();
        for data_stream in data_streams.data_streams {
            let ids = Identifiers {
                id: None,
                name: Some(data_stream.name.clone()),
                host: None,
                ip: None,
            };
            lookup_data_stream.insert(ids, data_stream);
        }
        log::debug!(
            "lookup_data_stream entries: {}",
            lookup_data_stream.entries.len(),
        );
        lookup_data_stream
    }
}
