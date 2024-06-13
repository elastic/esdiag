use super::{Identifiers, Lookup};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct DataStreamDoc {
    allow_custom_routing: Option<bool>,
    generation: u64,
    hidden: Option<bool>,
    ilm_policy: Option<String>,
    name: String,
    next_generation_managed_by: Option<String>,
    prefer_ilm: Option<bool>,
    replicated: Option<bool>,
    rollover_on_write: Option<bool>,
    status: String,
    system: Option<bool>,
    template: String,
    timestamp_field: TimestampField,
}

impl From<&DataStreamData> for DataStreamDoc {
    fn from(data_stream: &DataStreamData) -> Self {
        Self {
            allow_custom_routing: data_stream.allow_custom_routing,
            generation: data_stream.generation,
            hidden: data_stream.hidden,
            ilm_policy: data_stream.ilm_policy.clone(),
            name: data_stream.name.clone(),
            next_generation_managed_by: data_stream.next_generation_managed_by.clone(),
            prefer_ilm: data_stream.prefer_ilm,
            replicated: data_stream.replicated,
            rollover_on_write: data_stream.rollover_on_write,
            status: data_stream.status.clone(),
            system: data_stream.system.clone(),
            template: data_stream.template.clone(),
            timestamp_field: data_stream.timestamp_field.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DataStreamData {
    allow_custom_routing: Option<bool>,
    generation: u64,
    hidden: Option<bool>,
    ilm_policy: Option<String>,
    indices: Vec<Index>,
    name: String,
    next_generation_managed_by: Option<String>,
    prefer_ilm: Option<bool>,
    replicated: Option<bool>,
    rollover_on_write: Option<bool>,
    status: String,
    system: Option<bool>,
    template: String,
    timestamp_field: TimestampField,
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

impl From<&String> for Lookup<DataStreamDoc> {
    fn from(string: &String) -> Self {
        let data_streams: DataStreamWrapper =
            serde_json::from_str(&string).expect("Failed to parse DataStreamData");

        let mut lookup_data_stream: Lookup<DataStreamDoc> = Lookup::new();
        for data_stream in data_streams.data_streams {
            let data_stream_doc = DataStreamDoc::from(&data_stream);
            let indices: Vec<_> = data_stream
                .indices
                .into_iter()
                .map(|index| (index.index_name, index.index_uuid))
                .collect();

            let i = lookup_data_stream.append(data_stream_doc);
            for (name, uuid) in indices {
                let ids = Identifiers {
                    id: Some(uuid.clone()),
                    name: Some(name.clone()),
                    host: None,
                    ip: None,
                };
                lookup_data_stream.link(i, ids);
            }
        }
        log::debug!(
            "lookup_data_stream entries: {}",
            lookup_data_stream.entries.len(),
        );
        lookup_data_stream
    }
}
