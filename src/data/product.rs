// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{Application, Platform};
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

/// Legacy single-axis classification, superseded by the orthogonal
/// [`Platform`] and [`Application`] pair (ADR-0001).
///
/// This enum flattens the deployment platform and the application component
/// into one field and therefore cannot represent combinations like
/// Elasticsearch-on-ECK. It remains only as a transitional alias while call
/// sites migrate; do not add new variants or new concepts to it.
#[derive(Debug, Default, PartialEq, Hash, Clone, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    Agent,
    ECE,
    ECK,
    ElasticCloudHosted,
    #[default]
    Elasticsearch,
    Kibana,
    KubernetesPlatform,
    Logstash,
    Unknown,
}

impl std::fmt::Display for Product {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent => write!(fmt, "Agent"),
            Self::ECE => write!(fmt, "ECE"),
            Self::ECK => write!(fmt, "ECK"),
            Self::ElasticCloudHosted => write!(fmt, "ElasticCloudHosted"),
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::KubernetesPlatform => write!(fmt, "KubernetesPlatform"),
            Self::Logstash => write!(fmt, "Logstash"),
            Self::Unknown => write!(fmt, "Unknown"),
        }
    }
}

impl std::str::FromStr for Product {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "ece" => Ok(Self::ECE),
            "eck" => Ok(Self::ECK),
            "hosted" | "elastic-cloud-hosted" => Ok(Self::ElasticCloudHosted),
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            "mki" => Ok(Self::KubernetesPlatform),
            "unknown" => Ok(Self::Unknown),
            _ => Err("Unknown product".to_string()),
        }
    }
}

impl Product {
    /// Split the flattened legacy value into the orthogonal
    /// (platform, application) pair.
    ///
    /// Application variants carry no platform information, so their platform
    /// is `Unknown`; platform variants carry no application (`None`).
    pub fn split(&self) -> (Platform, Option<Application>) {
        match self {
            Self::Agent => (Platform::Unknown, Some(Application::Agent)),
            Self::Elasticsearch => (Platform::Unknown, Some(Application::Elasticsearch)),
            Self::Kibana => (Platform::Unknown, Some(Application::Kibana)),
            Self::Logstash => (Platform::Unknown, Some(Application::Logstash)),
            Self::ECE => (Platform::ECE, None),
            Self::ECK => (Platform::ECK, None),
            Self::ElasticCloudHosted => (Platform::ElasticCloudHosted, None),
            Self::KubernetesPlatform => (Platform::KubernetesPlatform, None),
            Self::Unknown => (Platform::Unknown, None),
        }
    }

    pub fn platform(&self) -> Platform {
        self.split().0
    }

    pub fn application(&self) -> Option<Application> {
        self.split().1
    }
}

impl From<Application> for Product {
    fn from(application: Application) -> Self {
        match application {
            Application::Elasticsearch => Self::Elasticsearch,
            Application::Kibana => Self::Kibana,
            Application::Logstash => Self::Logstash,
            Application::Agent => Self::Agent,
        }
    }
}

impl From<Platform> for Product {
    fn from(platform: Platform) -> Self {
        match platform {
            Platform::ECE => Self::ECE,
            Platform::ECK => Self::ECK,
            Platform::ElasticCloudHosted => Self::ElasticCloudHosted,
            Platform::KubernetesPlatform => Self::KubernetesPlatform,
            // The legacy axis cannot represent SelfManaged; it was implicit as
            // "an application with no platform wrapper".
            Platform::SelfManaged | Platform::Unknown => Self::Unknown,
        }
    }
}

/// Collapse the orthogonal pair back onto the legacy single axis: the
/// application wins when present, otherwise the platform.
impl From<(Platform, Option<Application>)> for Product {
    fn from((platform, application): (Platform, Option<Application>)) -> Self {
        match application {
            Some(application) => application.into(),
            None => platform.into(),
        }
    }
}

// Custom case-insensitve deserialization for the Product enum
impl<'de> Deserialize<'de> for Product {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize as a string
        let s: String = Deserialize::deserialize(deserializer)?;

        // Normalize the string to lowercase to match
        Product::from_str(&s.to_lowercase()).map_err(|e| serde::de::Error::custom(format!("Unknown product: {}", e)))
    }
}
