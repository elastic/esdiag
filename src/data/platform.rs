// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

/// The deployment environment a diagnostic was collected from.
///
/// Required and mutually exclusive: every diagnostic has exactly one platform,
/// and there is no "no platform" case — a bare install is `SelfManaged`.
/// Determined best-effort from indicators at the receiver; `Unknown` is the
/// escape hatch for indeterminate provenance (e.g. legacy `support-diagnostics`
/// bundles) and must be tolerated by every consumer.
#[derive(Debug, Default, PartialEq, Hash, Clone, Copy, Eq)]
pub enum Platform {
    SelfManaged,
    ElasticCloudHosted,
    ECE,
    ECK,
    KubernetesPlatform,
    #[default]
    Unknown,
}

impl Serialize for Platform {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

impl Platform {
    /// Stable hyphenated key for identifiers and selection strings.
    ///
    /// The cloud values match the legacy `orchestration` identifier strings so
    /// existing saved identifiers keep parsing.
    pub fn key(&self) -> &'static str {
        match self {
            Self::SelfManaged => "self-managed",
            Self::ElasticCloudHosted => "elastic-cloud-hosted",
            Self::ECE => "elastic-cloud-enterprise",
            Self::ECK => "elastic-cloud-kubernetes",
            Self::KubernetesPlatform => "kubernetes-platform",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelfManaged => write!(fmt, "SelfManaged"),
            Self::ElasticCloudHosted => write!(fmt, "ElasticCloudHosted"),
            Self::ECE => write!(fmt, "ECE"),
            Self::ECK => write!(fmt, "ECK"),
            Self::KubernetesPlatform => write!(fmt, "KubernetesPlatform"),
            Self::Unknown => write!(fmt, "Unknown"),
        }
    }
}

impl FromStr for Platform {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "selfmanaged" | "self-managed" | "self_managed" => Ok(Self::SelfManaged),
            "hosted" | "elastic-cloud-hosted" | "elasticcloudhosted" | "ech" => Ok(Self::ElasticCloudHosted),
            "ece" | "elastic-cloud-enterprise" => Ok(Self::ECE),
            "eck" | "elastic-cloud-kubernetes" => Ok(Self::ECK),
            "mki" | "kubernetes-platform" | "kubernetesplatform" => Ok(Self::KubernetesPlatform),
            "unknown" => Ok(Self::Unknown),
            _ => Err("Unknown platform".to_string()),
        }
    }
}

// Custom case-insensitive deserialization for the Platform enum
impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Platform::from_str(&s).map_err(|_| serde::de::Error::custom(format!("Unknown platform: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_legacy_orchestration_identifiers() {
        assert_eq!(Platform::from_str("elastic-cloud-kubernetes"), Ok(Platform::ECK));
        assert_eq!(Platform::from_str("elastic-cloud-enterprise"), Ok(Platform::ECE));
        assert_eq!(
            Platform::from_str("kubernetes-platform"),
            Ok(Platform::KubernetesPlatform)
        );
        assert_eq!(
            Platform::from_str("elastic-cloud-hosted"),
            Ok(Platform::ElasticCloudHosted)
        );
    }

    #[test]
    fn from_str_is_case_insensitive() {
        assert_eq!(Platform::from_str("ECK"), Ok(Platform::ECK));
        assert_eq!(Platform::from_str("SelfManaged"), Ok(Platform::SelfManaged));
    }

    #[test]
    fn default_is_unknown() {
        assert_eq!(Platform::default(), Platform::Unknown);
    }

    #[test]
    fn serde_round_trip() {
        for (platform, expected) in [
            (Platform::SelfManaged, "self-managed"),
            (Platform::ElasticCloudHosted, "elastic-cloud-hosted"),
            (Platform::ECE, "elastic-cloud-enterprise"),
            (Platform::ECK, "elastic-cloud-kubernetes"),
            (Platform::KubernetesPlatform, "kubernetes-platform"),
            (Platform::Unknown, "unknown"),
        ] {
            let json = serde_json::to_string(&platform).expect("serialize");
            assert_eq!(json, format!("\"{expected}\""));
            let parsed: Platform = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(parsed, platform);
        }
    }

    #[test]
    fn deserialize_error_includes_input() {
        let error = serde_json::from_str::<Platform>(r#""not-a-platform""#).expect_err("invalid platform");
        assert!(
            error.to_string().contains("Unknown platform: not-a-platform"),
            "unexpected error: {error}"
        );
    }
}
