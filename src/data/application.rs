// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

/// An Elastic Stack component that produces application-level diagnostic data.
///
/// A closed set of exactly four — never a platform value. Optional on a
/// diagnostic: a diagnostic carrying the platform's own data (e.g. an ECE
/// bundle, or the orchestration-level data of an ECK bundle) has no
/// application. A diagnostic's display label is its application if present,
/// else its platform.
#[derive(Debug, PartialEq, Hash, Clone, Copy, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Application {
    Elasticsearch,
    Kibana,
    Logstash,
    Agent,
}

impl Application {
    /// Stable lowercase key for identifiers and selection strings.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Elasticsearch => "elasticsearch",
            Self::Kibana => "kibana",
            Self::Logstash => "logstash",
            Self::Agent => "agent",
        }
    }
}

impl std::fmt::Display for Application {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::Logstash => write!(fmt, "Logstash"),
            Self::Agent => write!(fmt, "Agent"),
        }
    }
}

impl FromStr for Application {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            "agent" => Ok(Self::Agent),
            _ => Err("Unknown application".to_string()),
        }
    }
}

// Custom case-insensitive deserialization for the Application enum
impl<'de> Deserialize<'de> for Application {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Application::from_str(&s).map_err(|_| serde::de::Error::custom(format!("Unknown application: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_aliases() {
        assert_eq!(Application::from_str("es"), Ok(Application::Elasticsearch));
        assert_eq!(Application::from_str("kb"), Ok(Application::Kibana));
        assert_eq!(Application::from_str("ls"), Ok(Application::Logstash));
        assert_eq!(Application::from_str("Agent"), Ok(Application::Agent));
    }

    #[test]
    fn platform_values_are_rejected() {
        assert!(Application::from_str("eck").is_err());
        assert!(Application::from_str("ece").is_err());
        assert!(Application::from_str("kubernetes-platform").is_err());
        assert!(Application::from_str("elastic-cloud-hosted").is_err());
    }

    #[test]
    fn serde_round_trip() {
        for application in [
            Application::Elasticsearch,
            Application::Kibana,
            Application::Logstash,
            Application::Agent,
        ] {
            let json = serde_json::to_string(&application).expect("serialize");
            let parsed: Application = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, application);
        }
    }

    #[test]
    fn deserialize_error_includes_input() {
        let error = serde_json::from_str::<Application>(r#""not-an-application""#).expect_err("invalid application");
        assert!(
            error.to_string().contains("Unknown application: not-an-application"),
            "unexpected error: {error}"
        );
    }
}
