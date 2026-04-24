// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::Serialize;
use std::collections::HashMap;

/// A lookup table that allows for retrieving by four different keys: host, id, ip, and name
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Lookup<T> {
    by_id: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
    entries: Vec<T>,
    pub parsed: bool,
}

impl<T> Lookup<T>
where
    T: Clone + Serialize,
{
    pub fn new() -> Lookup<T> {
        Lookup {
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            entries: Vec::new(),
            parsed: false,
        }
    }

    pub fn was_parsed(mut self) -> Self {
        self.parsed = true;
        self
    }

    pub fn from_parsed<U>(value: U) -> Self
    where
        Self: From<U>,
    {
        Self::from(value).was_parsed()
    }
}

impl<T> Default for Lookup<T>
where
    T: Clone + Serialize,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Lookup<T>
where
    T: Clone + Serialize,
{
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

    /// Number of entries in the lookup table
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the lookup table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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

    /// Associate a name with the last entry added
    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.by_name.insert(name.to_string(), self.entries.len() - 1);
        self
    }
}

impl<T: Serialize> std::fmt::Display for Lookup<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}
