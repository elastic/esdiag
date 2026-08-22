// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::ValueEnum;
use eyre::Result;
use redact::Secret;
use std::str::FromStr;

/// Resolved credential material for one stage. The secret halves are wrapped
/// so neither `Debug` nor `Display` can print them (ADR-0011); reaching the
/// plaintext takes an explicit `expose_secret()` at the transport boundary.
#[derive(Clone, Debug)]
pub enum Auth {
    /// Use an API key authentication via headers
    Apikey(Secret<String>),
    /// Use username and password authentication via Basic Auth headers
    Basic(String, Secret<String>),
    /// Don't use any authentication
    None,
}

impl Auth {
    pub fn new(r#type: &AuthType, username: Option<String>, password: Option<String>, apikey: Option<String>) -> Self {
        match (r#type, username, password, apikey) {
            (AuthType::Apikey, _, _, Some(apikey)) => Self::apikey(apikey),
            (AuthType::Basic, Some(username), Some(password), _) => Self::basic(username, password),
            _ => Self::None,
        }
    }

    /// Wraps an API key that arrived as plaintext from a CLI flag, environment
    /// variable, form field, or the decrypted keystore.
    pub fn apikey(apikey: impl Into<String>) -> Self {
        Self::Apikey(Secret::new(apikey.into()))
    }

    /// As [`Auth::apikey`], for a username and password pair.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic(username.into(), Secret::new(password.into()))
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

#[cfg(test)]
mod tests {
    use super::Auth;

    #[test]
    fn auth_display_redacts_secret_material() {
        assert_eq!(Auth::apikey("ad-hoc-api-key").to_string(), "Apikey");
        assert_eq!(Auth::basic("elastic", "super-secret-password").to_string(), "Basic");
    }

    /// `Display` was redacted by hand while `Debug` was derived, so a `{:?}`
    /// anywhere near a credential used to print it verbatim.
    #[test]
    fn auth_debug_redacts_secret_material() {
        let apikey = format!("{:?}", Auth::apikey("ad-hoc-api-key"));
        let basic = format!("{:?}", Auth::basic("elastic", "super-secret-password"));

        assert!(!apikey.contains("ad-hoc-api-key"), "{apikey}");
        assert!(!basic.contains("super-secret-password"), "{basic}");
        assert!(basic.contains("elastic"), "the username is not secret: {basic}");
    }
}
