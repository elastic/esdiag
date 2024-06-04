use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DataStreamLookup {
    pub by_index: HashMap<String, Value>,
}

impl DataStreamLookup {
    pub fn new() -> DataStreamLookup {
        DataStreamLookup {
            by_index: HashMap::new(),
        }
    }

    pub fn from_value(data_streams: Value) -> DataStreamLookup {
        let mut data_stream_lookup = DataStreamLookup::new();

        for data_stream in data_streams["data_streams"].as_array().unwrap().clone() {
            for (i, index) in data_stream["indices"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .enumerate()
                .clone()
            {
                let last_index: usize = data_stream["indices"].as_array().unwrap().len() - 1;
                let is_write_index: bool = i == last_index;
                let mut data_stream_obj = data_stream.as_object().unwrap().clone();
                data_stream_obj.insert(
                    "is_write_index".to_string(),
                    serde_json::Value::Bool(is_write_index),
                );
                data_stream_obj.remove("indices");
                data_stream_lookup.by_index.insert(
                    index["index_name"].as_str().unwrap().into(),
                    serde_json::to_value(data_stream_obj).unwrap(),
                );
            }
        }
        data_stream_lookup
    }
}
