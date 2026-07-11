// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

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
