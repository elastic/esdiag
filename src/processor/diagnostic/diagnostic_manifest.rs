// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::Identifiers;
use super::{DiagPath, Manifest};
use crate::data::{Application, Platform, Product};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestedApi {
    /// Final HTTP response status observed for this API request, if available
    pub status: Option<u16>,
    /// Number of retry attempts performed before the final response
    #[serde(default)]
    pub retries: u32,
    /// Total time spent waiting for collected response bodies for this API
    pub response_time_ms: u64,
    /// Total size in bytes of collected response bodies for this API
    pub response_size_bytes: u64,
}

#[derive(Deserialize, Serialize)]
pub struct DiagnosticManifest {
    /// Diagnostic bundle variation
    pub mode: Option<String>,
    /// Elastic Stack component name (legacy single-axis value; kept on the
    /// wire for read/write compatibility — see `platform`/`application`)
    pub product: Product,
    /// Deployment platform the diagnostic was collected from (ADR-0001).
    /// Absent in older manifests; resolve through [`Self::platform`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<Platform>,
    /// Application component the diagnostic pertains to (ADR-0001). Absent
    /// for platform-only diagnostics and in older manifests; resolve through
    /// [`Self::application`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application: Option<Application>,
    /// Command-line flags used when running the diagnostic collector
    pub flags: Option<String>,
    /// Diagnostic collector version
    pub diagnostic: Option<String>,
    /// Diagnostic type (relates to product, not mode)
    pub r#type: Option<String>,
    /// Where the diagnostic was run from
    pub runner: Option<String>,
    /// Elastic Stack version
    pub version: Option<String>,
    /// Datetime when the diagnostic was collected
    #[serde(rename = "timestamp")]
    pub collection_date: String,
    /// Collection time in milliseconds since the Unix epoch
    pub collection_date_millis: Option<u64>,
    /// Platform diagnostic bundles can contain multiple diagnostics from different components
    pub included_diagnostics: Option<Vec<DiagPath>>,
    /// Name for human-readable IDs
    #[serde(skip_deserializing)]
    pub name: String,
    #[serde(skip_deserializing)]
    diagnostic_id: RwLock<Option<String>>,
    /// Additional identifiers not included in the diagnostic itself
    pub identifiers: Option<Identifiers>,
    /// APIs requested during this run keyed by API name
    pub requested_apis: Option<BTreeMap<String, RequestedApi>>,
    /// Deprecated compatibility list of API names collected during this run.
    #[serde(default)]
    pub collected_apis: Option<Vec<String>>,
}

impl Clone for DiagnosticManifest {
    fn clone(&self) -> Self {
        let diagnostic_id = if let Ok(id) = self.diagnostic_id.read() {
            RwLock::new(id.clone())
        } else {
            RwLock::new(None)
        };
        Self {
            mode: self.mode.clone(),
            product: self.product.clone(),
            platform: self.platform,
            application: self.application,
            flags: self.flags.clone(),
            diagnostic: self.diagnostic.clone(),
            r#type: self.r#type.clone(),
            runner: self.runner.clone(),
            version: self.version.clone(),
            collection_date: self.collection_date.clone(),
            collection_date_millis: self.collection_date_millis,
            included_diagnostics: self.included_diagnostics.clone(),
            name: self.name.clone(),
            diagnostic_id,
            identifiers: self.identifiers.clone(),
            requested_apis: self.requested_apis.clone(),
            collected_apis: self.collected_apis.clone(),
        }
    }
}

impl DiagnosticManifest {
    fn parse_collection_date_millis(collection_date: &str) -> Option<u64> {
        if let Ok(date) = DateTime::parse_from_rfc3339(collection_date) {
            u64::try_from(date.timestamp_millis()).ok()
        } else if let Ok(date) = DateTime::parse_from_str(collection_date, "%Y-%m-%dT%H:%M:%S%.3f%z") {
            u64::try_from(date.timestamp_millis()).ok()
        } else {
            None
        }
    }

    fn valid_collection_date_millis(collection_date_millis: u64) -> Option<u64> {
        if collection_date_millis == 0 {
            return None;
        }

        let millis = i64::try_from(collection_date_millis).ok()?;
        Utc.timestamp_millis_opt(millis).single()?;
        Some(collection_date_millis)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collection_date: String,
        diagnostic: Option<String>,
        flags: Option<String>,
        included_diagnostics: Option<Vec<DiagPath>>,
        mode: Option<String>,
        product: Product,
        r#type: Option<String>,
        runner: Option<String>,
        version: Option<String>,
    ) -> Self {
        let collection_date_millis = Some(Self::parse_collection_date_millis(&collection_date).unwrap_or_else(|| {
            tracing::warn!("Failed to parse collection date: {}", &collection_date);
            u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or_default()
        }));
        let diagnostic_id = RwLock::new(None);
        let name = r#type.clone().unwrap_or("diagnostic".to_string());

        let (platform, application) = product.split();
        // Unknown is not an explicit platform: leave it unset so receiver-level
        // detection (syscalls folder, cloud hints) can still resolve it.
        let platform = (platform != Platform::Unknown).then_some(platform);
        Self {
            collection_date,
            collection_date_millis,
            diagnostic,
            diagnostic_id,
            flags,
            included_diagnostics,
            identifiers: None,
            requested_apis: None,
            collected_apis: None,
            mode,
            name,
            product,
            platform,
            application,
            r#type,
            runner,
            version,
        }
    }

    /// The deployment platform this diagnostic was collected from.
    ///
    /// Prefers the explicit manifest field (esdiag-written bundles); falls
    /// back to indicator-based detection for older or third-party manifests.
    pub fn platform(&self) -> Platform {
        self.platform.unwrap_or_else(|| self.detect_platform_from_indicators())
    }

    /// The application component this diagnostic pertains to, if any.
    ///
    /// Prefers the explicit manifest field; falls back to the legacy
    /// single-axis `product` value for older manifests.
    pub fn application(&self) -> Option<Application> {
        self.application.or_else(|| self.product.application())
    }

    /// Set the resolved deployment platform, keeping the legacy `product`
    /// value coherent for older readers of written manifests.
    pub fn set_platform(&mut self, platform: Platform) {
        self.platform = Some(platform);
        if self.product == Product::Unknown && self.application.is_none() {
            self.product = Product::from(platform);
        }
    }

    /// Whether the manifest carries an explicit platform (as opposed to one
    /// that must be detected from indicators).
    pub fn has_explicit_platform(&self) -> bool {
        self.platform.is_some()
    }

    /// The stable key naming this diagnostic's type: the application key when
    /// present, else the platform key (the display-label rule of ADR-0001).
    pub fn type_key(&self) -> String {
        match self.application() {
            Some(application) => application.key().to_string(),
            None => self.platform().key().to_string(),
        }
    }

    /// Best-effort platform detection from manifest indicators (ADR-0001):
    /// a `runner` of `ece` implies ECE, the platform collectors' diagnostic
    /// types imply ECK / KubernetesPlatform, and the legacy single-axis
    /// `product` may itself carry a platform value. Anything indeterminate is
    /// `Unknown` — callers must tolerate it.
    fn detect_platform_from_indicators(&self) -> Platform {
        if let Some(runner) = self.runner.as_deref() {
            match runner.to_lowercase().as_str() {
                "ece" | "elastic-cloud-enterprise" => return Platform::ECE,
                "eck" | "eck-diagnostics" => return Platform::ECK,
                _ => {}
            }
        }
        match self.r#type.as_deref() {
            Some("eck-diagnostics") => return Platform::ECK,
            Some("k8s-platform-diagnostics") => return Platform::KubernetesPlatform,
            _ => {}
        }
        let (platform, _) = self.product.split();
        platform
    }

    pub fn collection_date_in_millis(&self) -> u64 {
        self.collection_date_millis
            .and_then(Self::valid_collection_date_millis)
            .unwrap_or_else(|| {
                Self::parse_collection_date_millis(&self.collection_date).unwrap_or_else(|| {
                    tracing::warn!("Failed to parse collection date: {}", &self.collection_date);
                    u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or_default()
                })
            })
    }

    pub fn diagnostic_id(&self, uuid: &str) -> String {
        let mut id = self
            .diagnostic_id
            .write()
            .expect("Failed to obtain write lock for diagnostic id");

        match id.as_ref() {
            Some(id) => id.clone(),
            None => {
                let collection_date_string = Utc
                    .timestamp_millis_opt(self.collection_date_in_millis() as i64)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339_opts(SecondsFormat::Secs, true);

                // Human readable ID
                *id = Some(format!(
                    "{}@{}~{}",
                    self.name,
                    &collection_date_string[..10], // Trim to only date
                    &uuid[..4]
                ));
                id.clone().unwrap()
            }
        }
    }

    pub fn with_name(self, name: String) -> Self {
        Self { name, ..self }
    }

    pub fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers: Some(identifiers),
            ..self
        }
    }

    pub fn with_requested_apis(self, requested_apis: BTreeMap<String, RequestedApi>) -> Self {
        let collected_apis = requested_apis
            .iter()
            .filter(|(_, api)| api.status.is_some_and(|status| (200..300).contains(&status)))
            .map(|(name, _)| name.clone())
            .collect();
        Self {
            requested_apis: Some(requested_apis),
            collected_apis: Some(collected_apis),
            ..self
        }
    }

    #[deprecated(note = "use with_requested_apis to include per-request metadata")]
    pub fn with_collected_apis(self, collected_apis: Vec<String>) -> Self {
        Self {
            collected_apis: Some(collected_apis),
            ..self
        }
    }
}

impl DiagnosticManifest {
    pub const FILENAME: &'static str = "diagnostic_manifest.json";
}

impl From<Manifest> for DiagnosticManifest {
    fn from(manifest: Manifest) -> Self {
        let product = match manifest.diag_type.as_deref() {
            Some("eck-diagnostics") => Product::ECK,
            Some("k8s-platform-diagnostics") => Product::KubernetesPlatform,
            _ => Product::Elasticsearch,
        };
        DiagnosticManifest::new(
            manifest.collection_date,
            manifest.diag_version,
            manifest.diagnostic_inputs,
            manifest.included_diagnostics,
            Some("compatible".to_string()),
            product,
            manifest.diag_type,
            manifest.runner,
            manifest
                .product_version
                .map(|v| v.original_value.map(|v| v.to_string()).unwrap_or_default()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticManifest, RequestedApi};
    use crate::data::{Application, Platform, Product};
    use std::collections::BTreeMap;

    fn manifest_with(product: Product, r#type: Option<&str>, runner: Option<&str>) -> DiagnosticManifest {
        DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-test".to_string()),
            None,
            None,
            Some("support".to_string()),
            product,
            r#type.map(str::to_string),
            runner.map(str::to_string),
            Some("8.19.3".to_string()),
        )
    }

    #[test]
    fn detects_ece_platform_from_runner_indicator() {
        let manifest = manifest_with(Product::Unknown, Some("api"), Some("ece"));
        assert_eq!(manifest.platform(), Platform::ECE);
        assert!(!manifest.has_explicit_platform());
    }

    #[test]
    fn detects_eck_platform_from_diag_type_indicator() {
        let manifest = manifest_with(Product::Unknown, Some("eck-diagnostics"), Some("unknown"));
        assert_eq!(manifest.platform(), Platform::ECK);
    }

    #[test]
    fn detects_kubernetes_platform_from_diag_type_indicator() {
        let manifest = manifest_with(Product::Unknown, Some("k8s-platform-diagnostics"), Some("unknown"));
        assert_eq!(manifest.platform(), Platform::KubernetesPlatform);
    }

    #[test]
    fn indeterminate_provenance_is_unknown() {
        // A legacy support-diagnostics bundle: no platform indicators at all
        let manifest = manifest_with(Product::Elasticsearch, Some("api"), Some("unknown"));
        assert_eq!(manifest.platform(), Platform::Unknown);
        assert_eq!(manifest.application(), Some(Application::Elasticsearch));
    }

    #[test]
    fn platform_product_yields_platform_only_classification() {
        let manifest = manifest_with(Product::ECK, Some("eck-diagnostics"), Some("esdiag"));
        assert!(manifest.has_explicit_platform());
        assert_eq!(manifest.platform(), Platform::ECK);
        assert_eq!(manifest.application(), None);
        assert_eq!(manifest.type_key(), "elastic-cloud-kubernetes");
    }

    #[test]
    fn set_platform_overrides_detection_and_wins_for_children() {
        let mut manifest = manifest_with(Product::Elasticsearch, Some("api"), Some("unknown"));
        manifest.set_platform(Platform::ECK);
        assert_eq!(manifest.platform(), Platform::ECK);
        // The application axis is untouched by platform propagation
        assert_eq!(manifest.application(), Some(Application::Elasticsearch));
        assert_eq!(manifest.type_key(), "elasticsearch");
    }

    #[test]
    fn explicit_platform_round_trips_through_serde() {
        let mut manifest = manifest_with(Product::Elasticsearch, Some("api"), Some("unknown"));
        manifest.set_platform(Platform::SelfManaged);
        let json = serde_json::to_string(&manifest).expect("serialize manifest");
        let parsed: DiagnosticManifest = serde_json::from_str(&json).expect("deserialize manifest");
        assert!(parsed.has_explicit_platform());
        assert_eq!(parsed.platform(), Platform::SelfManaged);
        assert_eq!(parsed.application(), Some(Application::Elasticsearch));
    }

    #[test]
    fn legacy_manifest_without_platform_fields_still_deserializes() {
        let manifest: DiagnosticManifest = serde_json::from_str(
            r#"{
              "mode": "support",
              "product": "elasticsearch",
              "diagnostic": "esdiag-0.16.0-SNAPSHOT",
              "type": "elasticsearch_diagnostic",
              "runner": "esdiag",
              "version": "8.19.3",
              "timestamp": "2026-04-25T20:52:09.948Z"
            }"#,
        )
        .expect("legacy manifest should deserialize");
        assert!(!manifest.has_explicit_platform());
        assert_eq!(manifest.platform(), Platform::Unknown);
        assert_eq!(manifest.application(), Some(Application::Elasticsearch));
    }

    #[test]
    fn new_sets_collection_date_millis_from_timestamp() {
        let manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.16.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );

        assert_eq!(manifest.collection_date_millis, Some(1_777_148_323_610));
    }

    #[test]
    fn parse_collection_date_millis_rejects_pre_epoch_timestamp() {
        assert_eq!(
            DiagnosticManifest::parse_collection_date_millis("1969-12-31T23:59:59Z"),
            None
        );
    }

    #[test]
    fn collection_date_in_millis_uses_stored_value_first() {
        let mut manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.16.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );
        manifest.collection_date = "not-a-date".to_string();

        assert_eq!(manifest.collection_date_in_millis(), 1_777_148_323_610);
    }

    #[test]
    fn collection_date_in_millis_treats_zero_as_missing() {
        let mut manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.16.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );
        manifest.collection_date_millis = Some(0);

        assert_eq!(manifest.collection_date_in_millis(), 1_777_148_323_610);
    }

    #[test]
    fn collection_date_in_millis_rejects_out_of_range_stored_value() {
        let mut manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.16.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        );
        manifest.collection_date_millis = Some(u64::MAX);

        assert_eq!(manifest.collection_date_in_millis(), 1_777_148_323_610);
        assert_eq!(
            manifest.diagnostic_id("abcd1234"),
            "elasticsearch_diagnostic@2026-04-25~abcd"
        );
    }

    #[test]
    fn requested_api_retries_defaults_for_legacy_manifests() {
        let manifest: DiagnosticManifest = serde_json::from_str(
            r#"{
              "mode": "support",
              "product": "elasticsearch",
              "flags": null,
              "diagnostic": "esdiag-0.16.0-SNAPSHOT",
              "type": "elasticsearch_diagnostic",
              "runner": "esdiag",
              "version": "6.8.23",
              "timestamp": "2026-04-25T20:52:09.948Z",
              "collection_date_millis": 1777150329948,
              "included_diagnostics": null,
              "name": "elasticsearch_diagnostic",
              "diagnostic_id": null,
              "identifiers": null,
              "requested_apis": {
                "nodes": {
                  "status": 200,
                  "response_time_ms": 191,
                  "response_size_bytes": 14005
                }
              }
            }"#,
        )
        .expect("legacy diagnostic_manifest.json should deserialize");

        let requested_apis = manifest.requested_apis.expect("requested APIs");
        let nodes = requested_apis.get("nodes").expect("nodes API metadata");
        assert_eq!(nodes.retries, 0);
        assert_eq!(nodes.status, Some(200));
        assert_eq!(nodes.response_time_ms, 191);
        assert_eq!(nodes.response_size_bytes, 14005);
    }

    #[test]
    fn collected_apis_deserializes_for_legacy_consumers() {
        let manifest: DiagnosticManifest = serde_json::from_str(
            r#"{
              "mode": "support",
              "product": "elasticsearch",
              "flags": null,
              "diagnostic": "esdiag-0.16.0-SNAPSHOT",
              "type": "elasticsearch_diagnostic",
              "runner": "esdiag",
              "version": "6.8.23",
              "timestamp": "2026-04-25T20:52:09.948Z",
              "collection_date_millis": 1777150329948,
              "included_diagnostics": null,
              "name": "elasticsearch_diagnostic",
              "diagnostic_id": null,
              "identifiers": null,
              "collected_apis": ["nodes"]
            }"#,
        )
        .expect("legacy diagnostic_manifest.json should deserialize");

        assert_eq!(manifest.collected_apis, Some(vec!["nodes".to_string()]));
    }

    #[test]
    fn requested_apis_serializes_deprecated_collected_apis_list() {
        let manifest = DiagnosticManifest::new(
            "2026-04-25T20:18:43.610Z".to_string(),
            Some("esdiag-0.16.0-SNAPSHOT".to_string()),
            None,
            None,
            Some("support".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.19.3".to_string()),
        )
        .with_requested_apis(BTreeMap::from([
            (
                "nodes".to_string(),
                RequestedApi {
                    status: Some(200),
                    retries: 0,
                    response_time_ms: 191,
                    response_size_bytes: 14005,
                },
            ),
            (
                "missing_api".to_string(),
                RequestedApi {
                    status: Some(404),
                    retries: 0,
                    response_time_ms: 12,
                    response_size_bytes: 42,
                },
            ),
        ]));

        let value = serde_json::to_value(&manifest).expect("serialize manifest");
        assert_eq!(value["requested_apis"]["nodes"]["status"], 200);
        assert_eq!(value["requested_apis"]["missing_api"]["status"], 404);
        assert_eq!(value["collected_apis"], serde_json::json!(["nodes"]));
    }
}
