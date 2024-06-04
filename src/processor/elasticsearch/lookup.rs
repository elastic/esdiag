mod alias;
mod data_stream;
mod node;
use super::EsDataSet::*;
use crate::input::DataSet;
use alias::AliasLookup;
use data_stream::DataStreamLookup;
use node::NodeLookup;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "lookup")]
pub enum Lookup {
    #[serde(rename = "alias")]
    AliasLookup(AliasLookup),
    #[serde(rename = "data_stream")]
    DataStreamLookup(DataStreamLookup),
    #[serde(rename = "node")]
    NodeLookup(NodeLookup),
}

impl Lookup {
    pub fn new(data: DataSet, value: Value) -> Lookup {
        match data {
            DataSet::Elasticsearch(Alias) => Lookup::AliasLookup(AliasLookup::from_value(value)),
            DataSet::Elasticsearch(DataStreams) => {
                Lookup::DataStreamLookup(DataStreamLookup::from_value(value))
            }
            DataSet::Elasticsearch(Nodes) => Lookup::NodeLookup(NodeLookup::from_value(value)),
            _ => panic!("ERROR: Invalid lookup source"),
        }
    }

    pub fn to_value(&self) -> Value {
        let json = match serde_json::to_string(&self) {
            Ok(json) => json,
            Err(e) => panic!("ERROR: Failed to convert lookup to JSON {}", e),
        };

        let value = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(e) => panic!("ERROR: Failed to convert lookup to Value {}", e),
        };
        value
    }

    pub fn by_id(&self, id: &str) -> Option<Value> {
        match self {
            Lookup::NodeLookup(lookup) => match lookup.by_id.get(id) {
                Some(index) => Some(serde_json::to_value(&lookup.nodes[*index]).unwrap()),
                None => None,
            },
            _ => None,
        }
    }

    pub fn by_index(&self, index: &str) -> Option<Value> {
        match self {
            Lookup::AliasLookup(lookup) => match lookup.by_index.get(index) {
                Some(alias) => Some(serde_json::to_value(alias).unwrap()),
                None => None,
            },
            Lookup::DataStreamLookup(lookup) => match lookup.by_index.get(index) {
                Some(data_stream) => Some(data_stream.clone()),
                None => None,
            },
            _ => None,
        }
    }
}
