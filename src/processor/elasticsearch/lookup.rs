pub mod alias;
pub mod data_stream;
pub mod index;
pub mod node;
use super::EsDataSet::*;
use crate::input::DataSet;
use alias::AliasLookup;
use data_stream::DataStreamLookup;
use index::IndexLookup;
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
    IndexLookup(IndexLookup),
}

impl Lookup {
    pub fn from_value(data_set: DataSet, value: Value) -> Lookup {
        match data_set {
            DataSet::Elasticsearch(Alias) => Lookup::AliasLookup(AliasLookup::from_value(value)),
            DataSet::Elasticsearch(DataStreams) => {
                Lookup::DataStreamLookup(DataStreamLookup::from_value(value))
            }
            DataSet::Elasticsearch(Nodes) => Lookup::NodeLookup(NodeLookup::from_value(value)),
            _ => panic!("ERROR: Invalid lookup source"),
        }
    }

    pub fn insert(&mut self, id: &String, value: &Value) {
        match self {
            Lookup::AliasLookup(_) => (),      //lookup.insert(id, value),
            Lookup::DataStreamLookup(_) => (), // lookup.insert(id, value),
            Lookup::NodeLookup(lookup) => lookup.insert(id, value),
            Lookup::IndexLookup(lookup) => lookup.insert(id, value),
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
                Some(index) => {
                    Some(serde_json::to_value(&lookup.nodes[*index]).expect("Failed to parse node"))
                }
                None => None,
            },
            _ => None,
        }
    }

    pub fn by_index(&self, index: &str) -> Option<Value> {
        match self {
            Lookup::AliasLookup(lookup) => match lookup.by_index.get(index) {
                Some(alias) => Some(serde_json::to_value(alias).expect("Failed to parse alias")),
                None => None,
            },
            Lookup::DataStreamLookup(lookup) => match lookup.by_index.get(index) {
                Some(data_stream) => Some(data_stream.clone()),
                None => None,
            },
            Lookup::IndexLookup(lookup) => match lookup.by_index.get(index) {
                Some(index) => Some(serde_json::to_value(index).expect("Failed to parse index")),
                None => None,
            },
            _ => None,
        }
    }
}
