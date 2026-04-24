// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::ValueEnum;
use eyre::Result;
use std::str::FromStr;

pub enum Auth {
    /// Use an API key authentication via headers
    Apikey(String),
    /// Use username and password authentication via Basic Auth headers
    Basic(String, String),
    /// Don't use any authentication
    None,
}

impl Auth {
    pub fn new(r#type: &AuthType, username: Option<String>, password: Option<String>, apikey: Option<String>) -> Self {
        match (r#type, username, password, apikey) {
            (AuthType::Apikey, _, _, Some(apikey)) => Self::Apikey(apikey),
            (AuthType::Basic, Some(username), Some(password), _) => Self::Basic(username, password),
            _ => Self::None,
        }
    }
}

impl std::fmt::Display for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Apikey(_) => write!(f, "Apikey"),
            Self::Basic(_, _) => write!(f, "Basic"),
            Self::None => write!(f, "None"),
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum AuthType {
    Apikey,
    Basic,
    None,
}

impl FromStr for AuthType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "apikey" => Ok(Self::Apikey),
            "basic" => Ok(Self::Basic),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}
