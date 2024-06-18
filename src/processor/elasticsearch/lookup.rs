pub mod alias;
pub mod data_stream;
pub mod ilm;
pub mod index;
pub mod node;
pub mod shared_cache;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lookup<T> {
    entries: Vec<T>,
    by_id: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
    by_host: HashMap<String, usize>,
    by_ip: HashMap<String, usize>,
    lookup: String,
}

impl<T> Lookup<T>
where
    T: Clone + Serialize + LookupDisplay,
{
    pub fn new() -> Lookup<T> {
        Lookup {
            entries: Vec::new(),
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            by_host: HashMap::new(),
            by_ip: HashMap::new(),
            lookup: String::from(T::display()),
        }
    }

    // Getters

    pub fn by_name(&self, name: &str) -> Option<&T> {
        match self.by_name.get(name) {
            Some(index) => Some(&self.entries[*index]),
            None => None,
        }
    }

    pub fn by_id(&self, id: &str) -> Option<&T> {
        match self.by_id.get(id) {
            Some(index) => Some(&self.entries[*index]),
            None => None,
        }
    }

    // pub fn by_host(&self, host: &str) -> Option<&T> {
    //     match self.by_host.get(host) {
    //         Some(index) => Some(&self.entries[*index]),
    //         None => None,
    //     }
    // }

    // pub fn by_ip(&self, ip: &str) -> Option<&T> {
    //     match self.by_ip.get(ip) {
    //         Some(index) => Some(&self.entries[*index]),
    //         None => None,
    //     }
    // }

    // Setters

    pub fn add(&mut self, value: T) -> &mut Self {
        self.entries.push(value);
        self
    }

    pub fn with_id(&mut self, id: &str) -> &mut Self {
        self.by_id.insert(id.to_string(), self.entries.len() - 1);
        self
    }

    pub fn with_ip(&mut self, ip: &str) -> &mut Self {
        self.by_ip.insert(ip.to_string(), self.entries.len() - 1);
        self
    }

    pub fn with_host(&mut self, host: &str) -> &mut Self {
        self.by_host
            .insert(host.to_string(), self.entries.len() - 1);
        self
    }

    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.by_name
            .insert(name.to_string(), self.entries.len() - 1);
        self
    }

    // Formatters

    pub fn to_value(&self) -> Value {
        let json = match serde_json::to_string(&self) {
            Ok(json) => json,
            Err(e) => panic!("ERROR: Failed to convert lookup to JSON {}", e),
        };

        match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(e) => panic!("ERROR: Failed to convert JSON to Value {}", e),
        }
    }
}

pub trait LookupDisplay {
    fn display() -> &'static str;
}
