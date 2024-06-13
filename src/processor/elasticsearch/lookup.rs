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
}

pub struct Identifiers {
    pub id: Option<String>,
    pub name: Option<String>,
    pub host: Option<String>,
    pub ip: Option<String>,
}

impl<T> Lookup<T>
where
    T: Clone + Serialize,
{
    pub fn new() -> Lookup<T> {
        Lookup {
            entries: Vec::new(),
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            by_host: HashMap::new(),
            by_ip: HashMap::new(),
        }
    }

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

    pub fn by_host(&self, host: &str) -> Option<&T> {
        match self.by_host.get(host) {
            Some(index) => Some(&self.entries[*index]),
            None => None,
        }
    }

    pub fn by_ip(&self, ip: &str) -> Option<&T> {
        match self.by_ip.get(ip) {
            Some(index) => Some(&self.entries[*index]),
            None => None,
        }
    }

    pub fn insert(&mut self, identifiers: Identifiers, entry: T) {
        let index = self.entries.len();
        if let Some(id) = identifiers.id {
            self.by_id.insert(id.clone(), index);
        }
        if let Some(name) = identifiers.name {
            self.by_name.insert(name.clone(), index);
        }
        if let Some(host) = identifiers.host {
            self.by_host.insert(host.clone(), index);
        }
        if let Some(ip) = identifiers.ip {
            self.by_ip.insert(ip.clone(), index);
        }
        self.entries.push(entry);
    }

    pub fn append(&mut self, entry: T) -> usize {
        let index = self.entries.len();
        self.entries.push(entry);
        index
    }

    pub fn link(&mut self, index: usize, identifiers: Identifiers) {
        if let Some(id) = identifiers.id {
            self.by_id.insert(id.clone(), index);
        }
        if let Some(name) = identifiers.name {
            self.by_name.insert(name.clone(), index);
        }
        if let Some(host) = identifiers.host {
            self.by_host.insert(host.clone(), index);
        }
        if let Some(ip) = identifiers.ip {
            self.by_ip.insert(ip.clone(), index);
        }
    }

    pub fn update_id(&mut self, id: &String, value: &T)
    where
        T: Clone,
    {
        if let Some(index) = self.by_id.get(id) {
            self.entries[*index] = value.clone();
        }
    }

    pub fn update_name(&mut self, name: &String, value: &T)
    where
        T: Clone,
    {
        if let Some(index) = self.by_name.get(name) {
            self.entries[*index] = value.clone();
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
}
