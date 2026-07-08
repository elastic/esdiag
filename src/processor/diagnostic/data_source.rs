// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::Product;
use eyre::{Result, eyre};
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub enum DataSourceError {
    UnsupportedVersion(Version),
    MissingSource(String, String),
}

impl std::fmt::Display for DataSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "API not supported on target version {}", v),
            Self::MissingSource(product, name) => {
                write!(
                    f,
                    "Source configuration missing for product: {}, name: {}",
                    product, name
                )
            }
        }
    }
}

impl std::error::Error for DataSourceError {}

#[derive(Clone, Debug, Default)]
pub struct SourceContext {
    pub product: &'static str,
    pub version: Option<Version>,
}

impl SourceContext {
    pub fn new(product: &'static str, version: Option<Version>) -> Self {
        Self { product, version }
    }
}

pub trait DataSource {
    fn name() -> String;
    fn aliases() -> Vec<&'static str> {
        Vec::new()
    }

    fn resolve_source_request_path(ctx: &SourceContext) -> Result<String> {
        let version = ctx
            .version
            .as_ref()
            .ok_or_else(|| eyre!("Version required for request path"))?;
        let name = Self::name();
        let aliases = Self::aliases();
        let (_, source_conf) = get_source(ctx.product, &name, &aliases)?;
        source_conf.get_url(version)
    }

    fn resolve_source_file_path(ctx: &SourceContext) -> Result<String> {
        let name = Self::name();
        let aliases = Self::aliases();
        let (matched_name, source_conf) = get_source(ctx.product, &name, &aliases)?;
        Ok(source_conf.get_file_path(matched_name))
    }

    fn resolve_source_extension(ctx: &SourceContext) -> Result<String> {
        let name = Self::name();
        let aliases = Self::aliases();
        let (_, source_conf) = get_source(ctx.product, &name, &aliases)?;
        Ok(source_conf.extension.as_deref().unwrap_or(".json").to_string())
    }

    fn candidate_source_file_paths(ctx: &SourceContext) -> Result<Vec<String>> {
        let name = Self::name();
        let aliases = Self::aliases();
        let mut paths = Vec::new();

        let (matched_name, source_conf) = get_source(ctx.product, &name, &aliases)?;
        paths.push(source_conf.get_file_path(matched_name));

        for alias in aliases {
            // An alias may be its own registry entry, or a legacy file name
            // whose registry key was renamed to the canonical key (ADR-0005);
            // in the latter case derive its path from the canonical entry.
            let path = match get_source(ctx.product, alias, &[]) {
                Ok((matched_name, source_conf)) => source_conf.get_file_path(matched_name),
                Err(_) => source_conf.get_file_path(alias),
            };
            if !paths.contains(&path) {
                paths.push(path);
            }
        }

        Ok(paths)
    }
}

pub fn source_product_key(product: &Product) -> Result<&'static str> {
    match product {
        Product::Elasticsearch => Ok("elasticsearch"),
        Product::Kibana => Ok("kibana"),
        Product::Logstash => Ok("logstash"),
        _ => Err(eyre!("sources.yml overrides are not supported for product {}", product)),
    }
}

pub trait StreamingDataSource: DataSource {
    type Item: Send + 'static;
    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: Deserializer<'de>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq)]
#[serde(untagged)]
pub enum VersionSource {
    Url(String),
    Structured(VersionSourceDetails),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Eq)]
pub struct VersionSourceDetails {
    pub url: String,
    #[serde(default)]
    pub spaceaware: bool,
    pub paginate: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedVersionSource {
    pub url: String,
    pub spaceaware: bool,
    pub paginate: Option<String>,
}

/// A collection-definition entry: one data source (ADR-0005).
///
/// The registry is the single source of truth for what to collect and how to
/// process it. Fields beyond `versions`/`extension`/`subdir`/`retry` are
/// ESDiag enrichments preserved across reconciliation from
/// `support-diagnostics` (ADR-0006).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq)]
pub struct Source {
    pub extension: Option<String>,
    pub subdir: Option<String>,
    /// Comma-separated tags; diagnostic-type membership (`minimal`,
    /// `standard`, `light`) derives from these.
    pub tags: Option<String>,
    /// Retry the request on failure during collect.
    #[serde(default)]
    pub retry: bool,
    /// Legacy upstream flag, currently informational.
    #[serde(default, rename = "showErrors")]
    pub show_errors: Option<bool>,
    /// Load this source imposes on the system it is pulled from (1 = cheap …
    /// 5 = expensive); governs collect concurrency only (ADR-0017).
    #[serde(default)]
    pub source_weight: Option<u8>,
    /// ESDiag CPU/time to transform this source (1 = cheap … 5 = expensive);
    /// governs processing concurrency only (ADR-0017).
    #[serde(default)]
    pub processing_weight: Option<u8>,
    /// The processor may consume this source as a stream.
    #[serde(default)]
    pub streamable: bool,
    /// A typed processor is registered for this source (its dispatch key ==
    /// this registry key == `DataSource::name()`). Absent/false means
    /// collect-only — a valid role, not a wiring gap.
    #[serde(default)]
    pub processable: bool,
    /// Present iff this source is a user-facing processing option; `true`
    /// means it cannot be deselected.
    #[serde(default)]
    pub required: Option<bool>,
    /// Processing options that must be selected along with this one.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Sources that must be collected along with this one.
    #[serde(default)]
    pub collect_dependencies: Vec<String>,
    pub versions: BTreeMap<String, VersionSource>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            extension: Some(String::from(".json")),
            subdir: None,
            tags: None,
            retry: false,
            show_errors: None,
            source_weight: None,
            processing_weight: None,
            streamable: false,
            processable: false,
            required: None,
            dependencies: Vec::new(),
            collect_dependencies: Vec::new(),
            versions: BTreeMap::new(),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.subdir {
            Some(subdir) => write!(fmt, "{}", subdir),
            None => Ok(()),
        }
    }
}

static SOURCES: OnceLock<HashMap<&'static str, HashMap<String, Source>>> = OnceLock::new();

fn embedded_sources_str(product: &str) -> Result<&'static str> {
    match product {
        "elasticsearch" => Ok(include_str!("../../../assets/elasticsearch/sources.yml")),
        "kibana" => Ok(include_str!("../../../assets/kibana/sources.yml")),
        "logstash" => Ok(include_str!("../../../assets/logstash/sources.yml")),
        other => Err(eyre!("Unsupported sources product: {}", other)),
    }
}

fn required_source_keys(product: &str) -> &'static [&'static str] {
    match product {
        "elasticsearch" => &["version"],
        "kibana" => &["kibana_status", "kibana_spaces"],
        "logstash" => &["logstash_node", "logstash_version"],
        _ => &[],
    }
}

fn parse_sources_content(label: &str, content: &str) -> Result<HashMap<String, Source>> {
    serde_yaml::from_str(content).map_err(|e| eyre!("Failed to parse {}: {}", label, e))
}

fn validate_sources_product(product: &str, sources: &HashMap<String, Source>, label: &str) -> Result<()> {
    let required = required_source_keys(product);
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|key| !sources.contains_key(*key))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(eyre!(
            "{} does not look like a valid {} sources.yml; missing required keys: {}",
            label,
            product,
            missing.join(", ")
        ))
    }
}

fn load_embedded_sources(
    override_product: Option<&str>,
    override_path: Option<&str>,
) -> Result<HashMap<&'static str, HashMap<String, Source>>> {
    let mut products = HashMap::new();

    for product in ["elasticsearch", "kibana", "logstash"] {
        let (label, content) = if override_product == Some(product) {
            let path = override_path.ok_or_else(|| eyre!("Override path missing for {}", product))?;
            (
                format!("override sources file at {}", path),
                std::fs::read_to_string(path)
                    .map_err(|e| eyre!("Failed to read override sources file at {}: {}", path, e))?,
            )
        } else {
            (
                format!("embedded {} sources.yml", product),
                embedded_sources_str(product)?.to_string(),
            )
        };

        let sources = parse_sources_content(&label, &content)?;
        validate_sources_product(product, &sources, &label)?;
        products.insert(product, sources);
    }

    Ok(products)
}

pub fn get_sources() -> &'static HashMap<&'static str, HashMap<String, Source>> {
    SOURCES.get_or_init(|| load_embedded_sources(None, None).expect("Valid embedded sources.yml files"))
}

pub fn init_sources(product: &str, override_path: String) -> Result<()> {
    let products = load_embedded_sources(Some(product), Some(&override_path))?;
    SOURCES
        .set(products)
        .map_err(|_| eyre!("Sources already initialized"))?;
    Ok(())
}

pub fn get_source<'a>(
    product: &str,
    name: &'a str,
    aliases: &[&'a str],
) -> std::result::Result<(&'a str, &'static Source), DataSourceError> {
    let sources = get_sources();
    let product_sources = sources
        .get(product)
        .ok_or_else(|| DataSourceError::MissingSource(product.to_string(), name.to_string()))?;
    if let Some(source) = product_sources.get(name) {
        return Ok((name, source));
    }
    for alias in aliases {
        if let Some(source) = product_sources.get(*alias) {
            return Ok((*alias, source));
        }
    }
    Err(DataSourceError::MissingSource(product.to_string(), name.to_string()))
}

pub fn get_product_sources(product: &str) -> Option<&'static HashMap<String, Source>> {
    get_sources().get(product)
}

pub fn get_source_keys(product: &str) -> Vec<String> {
    get_product_sources(product)
        .map(|sources| sources.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn get_source_keys_with_tag(product: &str, tag: &str) -> Vec<String> {
    get_product_sources(product)
        .map(|sources| {
            sources
                .iter()
                .filter_map(|(name, source)| source.has_tag(tag).then_some(name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Default graded weight when the registry does not set one explicitly: the
/// legacy binary mapping (ADR-0017 migration) — `light`-tagged sources are 1,
/// everything else 3, on a 1–5 scale.
const LEGACY_LIGHT_WEIGHT: u8 = 1;
const LEGACY_HEAVY_WEIGHT: u8 = 3;

impl Source {
    /// Load this source imposes on the system it is pulled from (1–5).
    /// Governs collect concurrency only (ADR-0017).
    pub fn source_weight(&self) -> u8 {
        self.source_weight.unwrap_or(if self.has_tag("light") {
            LEGACY_LIGHT_WEIGHT
        } else {
            LEGACY_HEAVY_WEIGHT
        })
    }

    /// ESDiag CPU/time to transform this source (1–5). Governs processing
    /// concurrency only (ADR-0017).
    pub fn processing_weight(&self) -> u8 {
        self.processing_weight.unwrap_or(LEGACY_LIGHT_WEIGHT)
    }
}

/// A dispatch-table entry claim used by [`validate_processable_registry`]: a
/// processable source's canonical key and the `DataSource::name()` of its
/// registered typed impl.
pub struct ProcessableClaim {
    pub key: &'static str,
    pub datasource_name: String,
}

/// Runtime validation of the key-alignment invariant (ADR-0005): every
/// registry entry marked `processable` has exactly one registered typed impl,
/// every dispatch-table key exists in the registry marked `processable`, and
/// each key equals its impl's `DataSource::name()`. A registry entry without
/// a typed impl must not be marked `processable` — collect-only is a valid
/// role, never a wiring gap.
pub fn validate_processable_registry(product: &str, claims: &[ProcessableClaim]) -> Result<()> {
    let sources =
        get_product_sources(product).ok_or_else(|| eyre!("No collection definition for product {}", product))?;

    let mut errors = Vec::new();
    for claim in claims {
        match sources.get(claim.key) {
            None => errors.push(format!(
                "dispatch key '{}' has no {} registry entry",
                claim.key, product
            )),
            Some(source) if !source.processable => errors.push(format!(
                "dispatch key '{}' is not marked processable in the {} registry",
                claim.key, product
            )),
            Some(_) => {}
        }
        if claim.key != claim.datasource_name {
            errors.push(format!(
                "dispatch key '{}' != DataSource::name() '{}'",
                claim.key, claim.datasource_name
            ));
        }
    }

    let claimed: Vec<&str> = claims.iter().map(|claim| claim.key).collect();
    for (key, source) in sources {
        if source.processable && !claimed.contains(&key.as_str()) {
            errors.push(format!(
                "registry entry '{}' is marked processable but has no registered impl",
                key
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(eyre!(
            "Collection registry key alignment failed for {}: {}",
            product,
            errors.join("; ")
        ))
    }
}

impl Source {
    pub fn get_file_path(&self, name: &str) -> String {
        let extension = self.extension.as_deref().unwrap_or(".json");
        match &self.subdir {
            Some(subdir) => format!("{}/{}{}", subdir, name, extension),
            None => format!("{}{}", name, extension),
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags
            .as_deref()
            .map(|tags| tags.split(',').any(|value| value.trim() == tag))
            .unwrap_or(false)
    }

    pub fn is_spaceaware(&self) -> bool {
        self.versions.values().any(|version| match version {
            VersionSource::Url(_) => false,
            VersionSource::Structured(details) => details.spaceaware,
        })
    }

    pub fn resolve_version(&self, version: &Version) -> Result<ResolvedVersionSource> {
        // Strip pre-release tags (like -SNAPSHOT) to ensure our broad semver matching logic
        // in sources.yml (e.g. ">= 7.0.0") matches properly. Standard semver treats ">= 7.0.0"
        // as NOT matching "8.0.0-SNAPSHOT" by default unless specifically asked to.
        let mut clean_version = version.clone();
        clean_version.pre = semver::Prerelease::EMPTY;

        // Ranges are stored in native Rust `semver` form: the upstream
        // NPM/Java dialect is normalized once, at reconciliation (ADR-0006).
        for (req_str, source) in &self.versions {
            let req =
                VersionReq::parse(req_str).map_err(|e| eyre!("Failed to parse version req '{}': {}", req_str, e))?;
            if req.matches(&clean_version) {
                return Ok(match source {
                    VersionSource::Url(url) => ResolvedVersionSource {
                        url: url.clone(),
                        spaceaware: false,
                        paginate: None,
                    },
                    VersionSource::Structured(details) => ResolvedVersionSource {
                        url: details.url.clone(),
                        spaceaware: details.spaceaware,
                        paginate: details.paginate.clone(),
                    },
                });
            }
        }
        Err(DataSourceError::UnsupportedVersion(version.clone()).into())
    }

    pub fn get_url(&self, version: &Version) -> Result<String> {
        Ok(self.resolve_version(version)?.url)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessableClaim, get_sources, validate_processable_registry};
    use semver::{Version, VersionReq};

    #[test]
    fn all_registry_version_ranges_parse_with_stock_semver() {
        // Ranges are normalized to native `semver` form at reconciliation
        // (ADR-0006); the runtime has no compatibility shim.
        for (product, sources) in get_sources() {
            for (key, source) in sources {
                for range in source.versions.keys() {
                    VersionReq::parse(range)
                        .unwrap_or_else(|e| panic!("{product}/{key} range '{range}' is not native semver: {e}"));
                }
            }
        }
    }

    #[test]
    fn validate_registry_rejects_unaligned_dispatch_key() {
        let claims = vec![ProcessableClaim {
            key: "not_a_registry_key",
            datasource_name: "not_a_registry_key".to_string(),
        }];
        let err = validate_processable_registry("elasticsearch", &claims).expect_err("unaligned key must fail");
        assert!(err.to_string().contains("no elasticsearch registry entry"));
    }

    #[test]
    fn validate_registry_rejects_name_mismatch() {
        let claims = vec![ProcessableClaim {
            key: "tasks",
            datasource_name: "task_list".to_string(),
        }];
        let err = validate_processable_registry("elasticsearch", &claims).expect_err("name mismatch must fail");
        assert!(err.to_string().contains("!= DataSource::name()"));
    }

    #[test]
    fn validate_registry_rejects_processable_entry_without_impl() {
        // Claim only a subset: the remaining processable entries must be
        // reported as missing impls.
        let claims = vec![ProcessableClaim {
            key: "tasks",
            datasource_name: "tasks".to_string(),
        }];
        let err = validate_processable_registry("elasticsearch", &claims).expect_err("missing impls must fail");
        assert!(err.to_string().contains("has no registered impl"));
    }

    #[test]
    fn collect_only_entry_is_not_a_wiring_gap() {
        let sources = get_sources().get("elasticsearch").unwrap();
        let cat = sources.get("cat_aliases").unwrap();
        assert!(!cat.processable);
        // A collect-only source needs no impl and passes validation implicitly
        // (it is simply absent from the claims and not marked processable).
    }

    #[test]
    fn test_semver_parsing_and_matching() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // Let's test a simple one, like aliases
        let alias = es_sources.get("cat_aliases").unwrap();

        let v_0_9 = Version::parse("0.9.0").unwrap();
        let v_5_0 = Version::parse("5.0.0").unwrap();
        let v_5_1_1 = Version::parse("5.1.1").unwrap();
        let v_6_0 = Version::parse("6.0.0").unwrap();

        assert_eq!(alias.get_url(&v_0_9).unwrap(), "/_cat/aliases?v");
        assert_eq!(alias.get_url(&v_5_0).unwrap(), "/_cat/aliases?v");
        assert_eq!(alias.get_url(&v_5_1_1).unwrap(), "/_cat/aliases?v&s=alias,index");
        assert_eq!(alias.get_url(&v_6_0).unwrap(), "/_cat/aliases?v&s=alias,index");
    }

    #[test]
    fn test_semver_snapshots() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // snapshot should strip prerelease
        let ilm = es_sources.get("ilm_explain").unwrap();

        let v_8 = Version::parse("8.0.0-SNAPSHOT").unwrap();
        assert_eq!(ilm.get_url(&v_8).unwrap(), "/*/_ilm/explain?human&expand_wildcards=all");
    }

    #[test]
    fn test_file_path_generation() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        let alias = es_sources.get("cat_aliases").unwrap();
        assert_eq!(alias.get_file_path("cat_aliases"), "cat/cat_aliases.txt");

        let tasks = es_sources.get("tasks").unwrap();
        assert_eq!(tasks.get_file_path("tasks"), "tasks.json"); // no subdir, default extension is json if missing from yaml
    }

    #[test]
    fn test_logstash_sources_are_loaded() {
        let sources = get_sources();
        let logstash_sources = sources.get("logstash").unwrap();
        assert!(logstash_sources.contains_key("logstash_node"));
        assert!(logstash_sources.contains_key("logstash_nodes_hot_threads_human"));
    }

    #[test]
    fn test_logstash_source_url_and_extension_resolution() {
        let sources = get_sources();
        let logstash_sources = sources.get("logstash").unwrap();

        let health = logstash_sources.get("logstash_health_report").unwrap();
        let v_8_15 = Version::parse("8.15.0").unwrap();
        let v_8_16 = Version::parse("8.16.0").unwrap();
        assert!(health.get_url(&v_8_15).is_err());
        assert_eq!(health.get_url(&v_8_16).unwrap(), "/_health_report");

        let hot_threads_human = logstash_sources.get("logstash_nodes_hot_threads_human").unwrap();
        assert_eq!(
            hot_threads_human.get_file_path("logstash_nodes_hot_threads_human"),
            "logstash_nodes_hot_threads_human.txt"
        );
    }

    #[test]
    fn test_product_specific_override_only_replaces_target_product() {
        let dir = tempfile::tempdir().expect("temp dir");
        let override_path = dir.path().join("sources.yml");
        std::fs::write(
            &override_path,
            r#"
logstash_node:
  versions:
    "> 5.0.0": "/custom_node"
logstash_version:
  versions:
    "> 5.0.0": "/custom_version"
"#,
        )
        .expect("write override");

        let products =
            super::load_embedded_sources(Some("logstash"), Some(override_path.to_str().expect("override path")))
                .expect("load sources");

        let es_sources = products.get("elasticsearch").unwrap();
        let logstash_sources = products.get("logstash").unwrap();
        assert!(es_sources.contains_key("version"));
        assert_eq!(
            logstash_sources
                .get("logstash_node")
                .unwrap()
                .get_url(&Version::parse("8.19.0").unwrap())
                .unwrap(),
            "/custom_node"
        );
    }

    #[test]
    fn test_product_specific_override_rejects_wrong_product_shape() {
        let dir = tempfile::tempdir().expect("temp dir");
        let override_path = dir.path().join("sources.yml");
        std::fs::write(
            &override_path,
            r#"
version:
  versions:
    "> 5.0.0": "/"
"#,
        )
        .expect("write override");

        let err = match super::load_embedded_sources(
            Some("logstash"),
            Some(override_path.to_str().expect("override path")),
        ) {
            Ok(_) => panic!("override should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("valid logstash sources.yml"));
    }

    #[test]
    fn test_kibana_structured_version_resolution() {
        let sources = get_sources();
        let kibana_sources = sources.get("kibana").unwrap();
        let alerts = kibana_sources.get("kibana_alerts").unwrap();

        let resolved = alerts.resolve_version(&Version::parse("8.19.0").unwrap()).unwrap();

        assert_eq!(resolved.url, "/api/alerts/_find");
        assert!(resolved.spaceaware);
        assert_eq!(resolved.paginate.as_deref(), Some("per_page"));
    }

    #[test]
    fn test_kibana_source_file_path_generation() {
        let sources = get_sources();
        let kibana_sources = sources.get("kibana").unwrap();
        let status = kibana_sources.get("kibana_status").unwrap();

        assert_eq!(status.get_file_path("kibana_status"), "kibana_status.json");
    }
}
