/// Lookups for Elasticsearch data
pub mod elasticsearch;

use serde::Serialize;
use std::collections::HashMap;

/// A lookup table that allows for retrieving by four different keys: host, id, ip, and name
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lookup<T> {
    by_host: HashMap<String, usize>,
    by_id: HashMap<String, usize>,
    by_ip: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
    entries: Vec<T>,
}

impl<T> Lookup<T>
where
    T: Clone + Serialize,
{
    pub fn new() -> Lookup<T> {
        Lookup {
            by_host: HashMap::new(),
            by_id: HashMap::new(),
            by_ip: HashMap::new(),
            by_name: HashMap::new(),
            entries: Vec::new(),
        }
    }

    /// Retrieve a lookup entry by name
    pub fn by_name(&self, name: &str) -> Option<&T> {
        match self.by_name.get(name) {
            Some(index) => Some(&self.entries[*index]),
            None => None,
        }
    }

    /// Retrieve a lookup entry by id
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

    /// Number of entries in the lookup table
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Push a new entry into the lookup table
    pub fn add(&mut self, value: T) -> &mut Self {
        self.entries.push(value);
        self
    }

    /// Associate an id with the last entry added
    pub fn with_id(&mut self, id: &str) -> &mut Self {
        self.by_id.insert(id.to_string(), self.entries.len() - 1);
        self
    }

    /// Associate an ip with the last entry added
    pub fn with_ip(&mut self, ip: &str) -> &mut Self {
        self.by_ip.insert(ip.to_string(), self.entries.len() - 1);
        self
    }

    /// Associate a hostname with the last entry added
    pub fn with_host(&mut self, host: &str) -> &mut Self {
        self.by_host
            .insert(host.to_string(), self.entries.len() - 1);
        self
    }

    /// Associate a name with the last entry added
    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.by_name
            .insert(name.to_string(), self.entries.len() - 1);
        self
    }
}

impl<T: Serialize> std::fmt::Display for Lookup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}
