use eyre::{Result, eyre};
use indexmap::IndexSet;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticType {
    Minimal,
    Standard,
    Support,
    Light,
}

impl std::str::FromStr for DiagnosticType {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "minimal" => Ok(DiagnosticType::Minimal),
            "standard" => Ok(DiagnosticType::Standard),
            "support" => Ok(DiagnosticType::Support),
            "light" => Ok(DiagnosticType::Light),
            _ => Err(eyre!("Invalid diagnostic type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ApiWeight {
    Light,
    Heavy,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ElasticsearchApi {
    AliasList,
    Cluster,
    ClusterSettings,
    DataStreams,
    HealthReport,
    IlmExplain,
    IlmPolicies,
    IndicesSettings,
    IndicesStats,
    Licenses,
    MappingStats,
    Nodes,
    NodesStats,
    PendingTasks,
    Repositories,
    SearchableSnapshotsCacheStats,
    SearchableSnapshotsStats,
    Snapshots,
    SlmPolicies,
    Tasks,
    Raw(String, ApiWeight),
}

impl ElasticsearchApi {
    pub fn weight(&self) -> ApiWeight {
        match self {
            ElasticsearchApi::AliasList => ApiWeight::Heavy,
            ElasticsearchApi::Cluster => ApiWeight::Light,
            ElasticsearchApi::ClusterSettings => ApiWeight::Light,
            ElasticsearchApi::DataStreams => ApiWeight::Heavy,
            ElasticsearchApi::HealthReport => ApiWeight::Light,
            ElasticsearchApi::IlmExplain => ApiWeight::Light,
            ElasticsearchApi::IlmPolicies => ApiWeight::Light,
            ElasticsearchApi::IndicesSettings => ApiWeight::Heavy,
            ElasticsearchApi::IndicesStats => ApiWeight::Heavy,
            ElasticsearchApi::Licenses => ApiWeight::Light,
            ElasticsearchApi::MappingStats => ApiWeight::Heavy,
            ElasticsearchApi::Nodes => ApiWeight::Light,
            ElasticsearchApi::NodesStats => ApiWeight::Heavy,
            ElasticsearchApi::PendingTasks => ApiWeight::Light,
            ElasticsearchApi::Repositories => ApiWeight::Light,
            ElasticsearchApi::SearchableSnapshotsCacheStats => ApiWeight::Light,
            ElasticsearchApi::SearchableSnapshotsStats => ApiWeight::Light,
            ElasticsearchApi::Snapshots => ApiWeight::Heavy,
            ElasticsearchApi::SlmPolicies => ApiWeight::Light,
            ElasticsearchApi::Tasks => ApiWeight::Heavy,
            ElasticsearchApi::Raw(_, weight) => weight.clone(),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ElasticsearchApi::AliasList => "alias",
            ElasticsearchApi::Cluster => "cluster",
            ElasticsearchApi::ClusterSettings => "cluster_settings",
            ElasticsearchApi::DataStreams => "data_streams",
            ElasticsearchApi::HealthReport => "health_report",
            ElasticsearchApi::IlmExplain => "ilm_explain",
            ElasticsearchApi::IlmPolicies => "ilm_policies",
            ElasticsearchApi::IndicesSettings => "indices_settings",
            ElasticsearchApi::IndicesStats => "indices_stats",
            ElasticsearchApi::Licenses => "licenses",
            ElasticsearchApi::MappingStats => "mapping_stats",
            ElasticsearchApi::Nodes => "nodes",
            ElasticsearchApi::NodesStats => "nodes_stats",
            ElasticsearchApi::PendingTasks => "pending_tasks",
            ElasticsearchApi::Repositories => "repositories",
            ElasticsearchApi::SearchableSnapshotsCacheStats => "searchable_snapshots_cache_stats",
            ElasticsearchApi::SearchableSnapshotsStats => "searchable_snapshots_stats",
            ElasticsearchApi::Snapshots => "snapshot",
            ElasticsearchApi::SlmPolicies => "slm_policies",
            ElasticsearchApi::Tasks => "tasks",
            ElasticsearchApi::Raw(name, _) => name.as_str(),
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "alias" => Ok(ElasticsearchApi::AliasList),
            "cluster" => Ok(ElasticsearchApi::Cluster),
            // `version` in sources.yml corresponds to `/` and maps to `version.json`,
            // which is the same typed datasource used by `cluster`.
            "version" => Ok(ElasticsearchApi::Cluster),
            "cluster_settings" => Ok(ElasticsearchApi::ClusterSettings),
            "data_streams" => Ok(ElasticsearchApi::DataStreams),
            "health_report" => Ok(ElasticsearchApi::HealthReport),
            "ilm_explain" => Ok(ElasticsearchApi::IlmExplain),
            "ilm_policies" => Ok(ElasticsearchApi::IlmPolicies),
            "indices_settings" => Ok(ElasticsearchApi::IndicesSettings),
            "indices_stats" => Ok(ElasticsearchApi::IndicesStats),
            "licenses" => Ok(ElasticsearchApi::Licenses),
            "mapping_stats" => Ok(ElasticsearchApi::MappingStats),
            "nodes" => Ok(ElasticsearchApi::Nodes),
            "nodes_stats" => Ok(ElasticsearchApi::NodesStats),
            "pending_tasks" => Ok(ElasticsearchApi::PendingTasks),
            "repositories" => Ok(ElasticsearchApi::Repositories),
            "searchable_snapshots_cache_stats" => {
                Ok(ElasticsearchApi::SearchableSnapshotsCacheStats)
            }
            "searchable_snapshots_stats" => Ok(ElasticsearchApi::SearchableSnapshotsStats),
            "snapshot" => Ok(ElasticsearchApi::Snapshots),
            "slm_policies" => Ok(ElasticsearchApi::SlmPolicies),
            "tasks" => Ok(ElasticsearchApi::Tasks),
            _ => {
                let weight = match crate::processor::diagnostic::data_source::get_source(
                    "elasticsearch",
                    s,
                    &[],
                ) {
                    Ok((_, source)) => {
                        if source.has_tag("light") {
                            ApiWeight::Light
                        } else {
                            ApiWeight::Heavy
                        }
                    }
                    Err(_) => return Err(eyre!("Invalid Elasticsearch API: {}", s)),
                };
                Ok(ElasticsearchApi::Raw(s.to_string(), weight))
            }
        }
    }
}

impl std::str::FromStr for ElasticsearchApi {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KibanaApi {
    Status,
    Spaces,
    Raw(String),
}

impl KibanaApi {
    pub fn as_str(&self) -> &str {
        match self {
            KibanaApi::Status => "kibana_status",
            KibanaApi::Spaces => "kibana_spaces",
            KibanaApi::Raw(name) => name.as_str(),
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        crate::processor::diagnostic::data_source::get_source("kibana", s, &[])
            .map_err(|_| eyre!("Invalid Kibana API: {}", s))?;
        match s {
            "kibana_status" => Ok(KibanaApi::Status),
            "kibana_spaces" => Ok(KibanaApi::Spaces),
            _ => Ok(KibanaApi::Raw(s.to_string())),
        }
    }
}

impl std::str::FromStr for KibanaApi {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LogstashApi {
    Node,
    NodeStats,
    Raw(String, ApiWeight),
}

impl LogstashApi {
    fn normalize_name(s: &str) -> Result<String> {
        let canonical = match s {
            "node" | "logstash_node" => "logstash_node",
            "node_stats" | "logstash_node_stats" => "logstash_node_stats",
            "plugins" | "logstash_plugins" => "logstash_plugins",
            "version" | "logstash_version" => "logstash_version",
            "health_report" | "logstash_health_report" => "logstash_health_report",
            "hot_threads" | "logstash_nodes_hot_threads" => "logstash_nodes_hot_threads",
            "hot_threads_human" | "logstash_nodes_hot_threads_human" => {
                "logstash_nodes_hot_threads_human"
            }
            other => other,
        };

        crate::processor::diagnostic::data_source::get_source("logstash", canonical, &[])
            .map_err(|_| eyre!("Invalid Logstash API: {}", s))?;
        Ok(canonical.to_string())
    }

    pub fn weight(&self) -> ApiWeight {
        match self {
            LogstashApi::Node => ApiWeight::Light,
            LogstashApi::NodeStats => ApiWeight::Heavy,
            LogstashApi::Raw(_, weight) => weight.clone(),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            LogstashApi::Node => "logstash_node",
            LogstashApi::NodeStats => "logstash_node_stats",
            LogstashApi::Raw(name, _) => name.as_str(),
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let canonical = Self::normalize_name(s)?;
        match canonical.as_str() {
            "logstash_node" => Ok(LogstashApi::Node),
            "logstash_node_stats" => Ok(LogstashApi::NodeStats),
            _ => {
                let weight = match crate::processor::diagnostic::data_source::get_source(
                    "logstash",
                    canonical.as_str(),
                    &[],
                ) {
                    Ok((_, source)) => {
                        if source.tags.as_deref() == Some("light") {
                            ApiWeight::Light
                        } else {
                            ApiWeight::Heavy
                        }
                    }
                    Err(_) => return Err(eyre!("Invalid Logstash API: {}", s)),
                };
                Ok(LogstashApi::Raw(canonical, weight))
            }
        }
    }
}

impl std::str::FromStr for LogstashApi {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

pub struct ApiResolver;

#[derive(Debug, Clone, Serialize)]
pub struct ProcessingOption {
    pub key: String,
    pub required: bool,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ProcessSelection {
    pub product: String,
    pub diagnostic_type: String,
    pub selected: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProcessingOptionDef {
    key: &'static str,
    required: bool,
    dependencies: &'static [&'static str],
}

impl ApiResolver {
    fn resolve_requested(
        mut requested: IndexSet<String>,
        minimum_required: &[&str],
        dependencies: &HashMap<String, Vec<String>>,
    ) -> IndexSet<String> {
        for req in minimum_required {
            requested.insert((*req).to_string());
        }

        let mut final_set: IndexSet<String> = IndexSet::new();

        fn resolve_deps(
            api: &str,
            deps_map: &HashMap<String, Vec<String>>,
            final_set: &mut IndexSet<String>,
            visited: &mut IndexSet<String>,
        ) {
            if visited.contains(api) {
                return;
            }
            visited.insert(api.to_string());
            if let Some(api_deps) = deps_map.get(api) {
                for dep in api_deps {
                    resolve_deps(dep, deps_map, final_set, visited);
                }
            }
            final_set.insert(api.to_string());
        }

        let mut visited = IndexSet::new();
        for api in requested.iter() {
            resolve_deps(api, dependencies, &mut final_set, &mut visited);
        }

        final_set
    }

    pub fn es_minimum_required() -> Vec<&'static str> {
        vec!["cluster"]
    }

    pub fn kb_minimum_required() -> Vec<&'static str> {
        vec!["kibana_status", "kibana_spaces"]
    }

    pub fn ls_minimum_required() -> Vec<&'static str> {
        vec!["logstash_node"]
    }

    pub fn es_dependencies() -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();
        deps.insert("nodes_stats".to_string(), vec!["nodes".to_string()]);
        deps.insert("nodes".to_string(), vec!["cluster_settings".to_string()]);
        deps
    }

    pub fn kb_dependencies(requested: &IndexSet<String>) -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();
        for api in requested {
            if let Ok((_, source)) =
                crate::processor::diagnostic::data_source::get_source("kibana", api, &[])
                && source.is_spaceaware()
            {
                deps.entry(api.clone())
                    .or_insert_with(Vec::new)
                    .push("kibana_spaces".to_string());
            }
        }
        deps
    }

    pub fn ls_dependencies() -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();
        deps.insert(
            "logstash_node".to_string(),
            vec![
                "logstash_version".to_string(),
                "logstash_plugins".to_string(),
            ],
        );
        deps.insert(
            "logstash_node_stats".to_string(),
            vec!["logstash_node".to_string()],
        );
        deps
    }

    fn es_processing_defs() -> &'static [ProcessingOptionDef] {
        &[
            ProcessingOptionDef { key: "version", required: true, dependencies: &[] },
            ProcessingOptionDef {
                key: "cluster_settings_defaults",
                required: true,
                dependencies: &["version"],
            },
            ProcessingOptionDef {
                key: "cluster_settings",
                required: false,
                dependencies: &["version", "cluster_settings_defaults"],
            },
            ProcessingOptionDef { key: "health_report", required: false, dependencies: &["version"] },
            ProcessingOptionDef { key: "ilm_policies", required: false, dependencies: &["version"] },
            ProcessingOptionDef {
                key: "indices_settings",
                required: false,
                dependencies: &["version", "cluster_settings_defaults"],
            },
            ProcessingOptionDef {
                key: "indices_stats",
                required: false,
                dependencies: &["version", "cluster_settings_defaults"],
            },
            ProcessingOptionDef { key: "nodes", required: false, dependencies: &["version"] },
            ProcessingOptionDef {
                key: "nodes_stats",
                required: false,
                dependencies: &["nodes", "cluster_settings_defaults", "version"],
            },
            ProcessingOptionDef { key: "pending_tasks", required: false, dependencies: &["version"] },
            ProcessingOptionDef { key: "repositories", required: false, dependencies: &["version"] },
            ProcessingOptionDef {
                key: "slm_policies",
                required: false,
                dependencies: &["repositories", "version"],
            },
            ProcessingOptionDef { key: "snapshot", required: false, dependencies: &["repositories", "version"] },
            ProcessingOptionDef { key: "tasks", required: false, dependencies: &["nodes", "version"] },
        ]
    }

    fn ls_processing_defs() -> &'static [ProcessingOptionDef] {
        &[
            ProcessingOptionDef { key: "version", required: true, dependencies: &[] },
            ProcessingOptionDef { key: "plugins", required: true, dependencies: &["version"] },
            ProcessingOptionDef { key: "node", required: true, dependencies: &["version"] },
            ProcessingOptionDef { key: "node_stats", required: false, dependencies: &["node", "version"] },
        ]
    }

    pub fn resolve_processing_options(
        product: &str,
        diagnostic_type: &str,
        selected_csv: &str,
    ) -> Result<Vec<ProcessingOption>> {
        let requested: Vec<String> = selected_csv
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();
        let selected = Self::resolve_processing_selection(product, diagnostic_type, &requested)?;
        let defs = Self::processing_defs(product)?;

        Ok(defs
            .iter()
            .map(|def| ProcessingOption {
                key: def.key.to_string(),
                required: def.required,
                selected: selected.iter().any(|value| value == def.key),
            })
            .collect())
    }

    pub fn resolve_processing_selection(
        product: &str,
        diagnostic_type: &str,
        selected: &[String],
    ) -> Result<Vec<String>> {
        let defs = Self::processing_defs(product)?;
        let diag_type = DiagnosticType::from_str(diagnostic_type)?;
        let defaults = Self::default_processing_selection(product, &diag_type)?;
        let mut requested: IndexSet<String> = if selected.is_empty() {
            defaults.into_iter().collect()
        } else {
            selected.iter().cloned().collect()
        };

        for def in defs {
            if def.required {
                requested.insert(def.key.to_string());
            }
        }

        let mut resolved: IndexSet<String> = IndexSet::new();
        let mut visited = IndexSet::new();
        for key in requested {
            Self::resolve_processing_deps(product, &key, &mut resolved, &mut visited)?;
        }

        Ok(resolved.into_iter().collect())
    }

    pub fn processing_catalog() -> Result<HashMap<String, HashMap<String, Vec<ProcessingOption>>>> {
        let mut catalog = HashMap::new();
        for product in ["elasticsearch", "logstash"] {
            let mut by_type = HashMap::new();
            for diag_type in ["minimal", "light", "standard", "support"] {
                by_type.insert(
                    diag_type.to_string(),
                    Self::resolve_processing_options(product, diag_type, "")?,
                );
            }
            catalog.insert(product.to_string(), by_type);
        }
        Ok(catalog)
    }

    fn processing_defs(product: &str) -> Result<&'static [ProcessingOptionDef]> {
        match product {
            "elasticsearch" => Ok(Self::es_processing_defs()),
            "logstash" => Ok(Self::ls_processing_defs()),
            _ => Err(eyre!("Unsupported processing product: {}", product)),
        }
    }

    fn default_processing_selection(product: &str, diag_type: &DiagnosticType) -> Result<Vec<String>> {
        match product {
            "elasticsearch" => {
                let selected = Self::resolve_es(diag_type, None, None)?
                    .into_iter()
                    .map(|api| api.as_str().to_string())
                    .filter(|key| Self::es_processing_defs().iter().any(|def| def.key == key))
                    .collect();
                Ok(selected)
            }
            "logstash" => {
                let selected = Self::resolve_ls(diag_type, None, None)?
                    .into_iter()
                    .map(|api| api.as_str().to_string())
                    .filter(|key| Self::ls_processing_defs().iter().any(|def| def.key == key))
                    .collect();
                Ok(selected)
            }
            _ => Err(eyre!("Unsupported processing product: {}", product)),
        }
    }

    fn resolve_processing_deps(
        product: &str,
        key: &str,
        resolved: &mut IndexSet<String>,
        visited: &mut IndexSet<String>,
    ) -> Result<()> {
        if visited.contains(key) {
            return Ok(());
        }
        visited.insert(key.to_string());
        let def = Self::processing_defs(product)?
            .iter()
            .find(|def| def.key == key)
            .ok_or_else(|| eyre!("Unsupported processing option '{}' for {}", key, product))?;

        for dep in def.dependencies {
            Self::resolve_processing_deps(product, dep, resolved, visited)?;
        }
        resolved.insert(key.to_string());
        Ok(())
    }

    pub fn es_base_apis(diag_type: &DiagnosticType) -> Vec<String> {
        match diag_type {
            DiagnosticType::Minimal => vec!["cluster".to_string(), "nodes".to_string()],
            DiagnosticType::Standard => vec![
                "alias".to_string(),
                "cluster".to_string(),
                "cluster_settings".to_string(),
                "data_streams".to_string(),
                "health_report".to_string(),
                "ilm_explain".to_string(),
                "ilm_policies".to_string(),
                "indices_settings".to_string(),
                "indices_stats".to_string(),
                "licenses".to_string(),
                "mapping_stats".to_string(),
                "nodes".to_string(),
                "nodes_stats".to_string(),
                "pending_tasks".to_string(),
                "repositories".to_string(),
                "searchable_snapshots_cache_stats".to_string(),
                "searchable_snapshots_stats".to_string(),
                "snapshot".to_string(),
                "slm_policies".to_string(),
                "tasks".to_string(),
            ],
            DiagnosticType::Support => {
                crate::processor::diagnostic::data_source::get_source_keys("elasticsearch")
            }
            DiagnosticType::Light => {
                let mut light_apis =
                    crate::processor::diagnostic::data_source::get_source_keys_with_tag(
                        "elasticsearch",
                        "light",
                    );
                if !light_apis.iter().any(|api| api == "cluster") {
                    light_apis.push("cluster".to_string());
                }
                if !light_apis.iter().any(|api| api == "nodes") {
                    light_apis.push("nodes".to_string());
                }
                light_apis
            }
        }
    }

    pub fn kb_base_apis(diag_type: &DiagnosticType) -> Vec<String> {
        match diag_type {
            DiagnosticType::Minimal => Self::kb_minimum_required()
                .into_iter()
                .map(str::to_string)
                .collect(),
            DiagnosticType::Standard | DiagnosticType::Support | DiagnosticType::Light => {
                crate::processor::diagnostic::data_source::get_source_keys("kibana")
            }
        }
    }

    pub fn resolve_es(
        diag_type: &DiagnosticType,
        include: Option<&Vec<String>>,
        exclude: Option<&Vec<String>>,
    ) -> Result<Vec<ElasticsearchApi>> {
        let mut requested: IndexSet<String> = IndexSet::new();

        for api in Self::es_base_apis(diag_type) {
            requested.insert(api.to_string());
        }

        if let Some(incs) = include {
            for api in incs {
                ElasticsearchApi::parse(api)?;
                requested.insert(api.to_string());
            }
        }

        if let Some(excs) = exclude {
            for api in excs {
                ElasticsearchApi::parse(api)?;
                requested.swap_remove(api);
            }
        }

        let final_set = Self::resolve_requested(
            requested,
            &Self::es_minimum_required(),
            &Self::es_dependencies(),
        );

        let mut api_set: IndexSet<ElasticsearchApi> = IndexSet::new();
        for api in final_set.iter() {
            api_set.insert(ElasticsearchApi::parse(api)?);
        }

        let apis: Vec<ElasticsearchApi> = api_set.into_iter().collect();
        Ok(apis)
    }

    pub fn resolve_kb(
        diag_type: &DiagnosticType,
        include: Option<&Vec<String>>,
        exclude: Option<&Vec<String>>,
    ) -> Result<Vec<KibanaApi>> {
        let mut requested: IndexSet<String> = IndexSet::new();

        for api in Self::kb_base_apis(diag_type) {
            requested.insert(api);
        }

        if let Some(incs) = include {
            for api in incs {
                KibanaApi::parse(api)?;
                requested.insert(api.to_string());
            }
        }

        if let Some(excs) = exclude {
            for api in excs {
                KibanaApi::parse(api)?;
                requested.swap_remove(api);
            }
        }

        let deps = Self::kb_dependencies(&requested);
        let final_set = Self::resolve_requested(requested, &Self::kb_minimum_required(), &deps);

        let mut api_set: IndexSet<KibanaApi> = IndexSet::new();
        for api in final_set.iter() {
            api_set.insert(KibanaApi::parse(api)?);
        }

        Ok(api_set.into_iter().collect())
    }

    pub fn resolve_ls(
        diag_type: &DiagnosticType,
        include: Option<&Vec<String>>,
        exclude: Option<&Vec<String>>,
    ) -> Result<Vec<LogstashApi>> {
        let mut requested: IndexSet<String> = IndexSet::new();

        let base_apis: Vec<String> = match diag_type {
            DiagnosticType::Minimal => vec!["logstash_node".to_string()],
            DiagnosticType::Standard | DiagnosticType::Light => vec![
                "logstash_node".to_string(),
                "logstash_node_stats".to_string(),
            ],
            DiagnosticType::Support => {
                let sources = crate::processor::diagnostic::data_source::get_sources();
                if let Some(logstash_sources) = sources.get("logstash") {
                    logstash_sources.keys().cloned().collect()
                } else {
                    vec![]
                }
            }
        };

        for api in base_apis {
            requested.insert(api);
        }

        if let Some(incs) = include {
            for api in incs {
                let normalized = LogstashApi::normalize_name(api)?;
                requested.insert(normalized);
            }
        }

        if let Some(excs) = exclude {
            for api in excs {
                let normalized = LogstashApi::normalize_name(api)?;
                requested.swap_remove(&normalized);
            }
        }

        let final_set = Self::resolve_requested(
            requested,
            &Self::ls_minimum_required(),
            &Self::ls_dependencies(),
        );

        let mut apis = Vec::new();
        for api in final_set.iter() {
            apis.push(LogstashApi::parse(api)?);
        }

        Ok(apis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_es_resolve_minimal_dependencies() {
        let apis = ApiResolver::resolve_es(
            &DiagnosticType::Minimal,
            Some(&vec!["nodes_stats".to_string()]),
            None,
        )
        .unwrap();

        let api_strs: Vec<&str> = apis.iter().map(|a| a.as_str()).collect();
        assert!(api_strs.contains(&"cluster")); // required
        assert!(api_strs.contains(&"nodes")); // resolved as dependency of nodes_stats
        assert!(api_strs.contains(&"nodes_stats")); // explicitly included
        assert!(api_strs.contains(&"cluster_settings")); // resolved as dependency of nodes
    }

    #[test]
    fn test_es_resolve_exclude_required() {
        let apis = ApiResolver::resolve_es(
            &DiagnosticType::Standard,
            None,
            Some(&vec!["cluster".to_string()]),
        )
        .unwrap();

        let api_strs: Vec<&str> = apis.iter().map(|a| a.as_str()).collect();
        assert!(api_strs.contains(&"cluster")); // Should still be there because it's required
    }

    #[test]
    fn test_es_invalid_include() {
        let res = ApiResolver::resolve_es(
            &DiagnosticType::Standard,
            Some(&vec!["not_a_real_api".to_string()]),
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_es_deduplication() {
        let apis = ApiResolver::resolve_es(
            &DiagnosticType::Minimal,
            Some(&vec!["nodes".to_string(), "nodes".to_string()]),
            None,
        )
        .unwrap();

        let api_strs: Vec<&str> = apis.iter().map(|a| a.as_str()).collect();
        assert_eq!(api_strs.iter().filter(|&&x| x == "nodes").count(), 1);
    }

    #[test]
    fn test_es_version_alias_dedupes_to_cluster() {
        let apis = ApiResolver::resolve_es(
            &DiagnosticType::Minimal,
            Some(&vec!["version".to_string()]),
            None,
        )
        .unwrap();

        let cluster_count = apis
            .iter()
            .filter(|api| matches!(api, ElasticsearchApi::Cluster))
            .count();
        assert_eq!(cluster_count, 1);
    }

    #[test]
    fn test_processing_selection_locks_required_dependencies() {
        let selected = ApiResolver::resolve_processing_selection(
            "elasticsearch",
            "minimal",
            &["nodes_stats".to_string()],
        )
        .unwrap();

        assert!(selected.contains(&"nodes_stats".to_string()));
        assert!(selected.contains(&"nodes".to_string()));
        assert!(selected.contains(&"version".to_string()));
        assert!(selected.contains(&"cluster_settings_defaults".to_string()));
    }

    #[test]
    fn test_processing_options_marks_required_entries() {
        let options =
            ApiResolver::resolve_processing_options("logstash", "standard", "").unwrap();

        let version = options.iter().find(|option| option.key == "version").unwrap();
        let plugins = options.iter().find(|option| option.key == "plugins").unwrap();
        let node_stats = options
            .iter()
            .find(|option| option.key == "node_stats")
            .unwrap();

        assert!(version.required);
        assert!(plugins.required);
        assert!(node_stats.selected);
    }
}
