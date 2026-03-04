use eyre::{Result, eyre};
use indexmap::IndexSet;
use std::collections::HashMap;

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
                        if source.tags.as_deref() == Some("light") {
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
pub enum LogstashApi {
    Node,
    NodeStats,
}

impl LogstashApi {
    pub fn weight(&self) -> ApiWeight {
        match self {
            LogstashApi::Node => ApiWeight::Light,
            LogstashApi::NodeStats => ApiWeight::Heavy,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            LogstashApi::Node => "node",
            LogstashApi::NodeStats => "node_stats",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "node" => Ok(LogstashApi::Node),
            "node_stats" => Ok(LogstashApi::NodeStats),
            _ => Err(eyre!("Invalid Logstash API: {}", s)),
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

impl ApiResolver {
    pub fn es_minimum_required() -> Vec<&'static str> {
        vec!["cluster"]
    }

    pub fn ls_minimum_required() -> Vec<&'static str> {
        vec!["node"]
    }

    pub fn es_dependencies() -> HashMap<&'static str, Vec<&'static str>> {
        let mut deps = HashMap::new();
        deps.insert("nodes_stats", vec!["nodes"]);
        deps.insert("nodes", vec!["cluster_settings"]);
        deps
    }

    pub fn ls_dependencies() -> HashMap<&'static str, Vec<&'static str>> {
        let mut deps = HashMap::new();
        deps.insert("node_stats", vec!["node"]);
        deps
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
                let sources = crate::processor::diagnostic::data_source::get_sources();
                if let Some(es_sources) = sources.get("elasticsearch") {
                    es_sources.keys().cloned().collect()
                } else {
                    vec![]
                }
            }
            DiagnosticType::Light => {
                let sources = crate::processor::diagnostic::data_source::get_sources();
                if let Some(es_sources) = sources.get("elasticsearch") {
                    let mut light_apis: Vec<String> = es_sources
                        .iter()
                        .filter_map(|(k, v)| {
                            if v.tags.as_deref() == Some("light") {
                                Some(k.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    // Ensure minimums are included
                    if !light_apis.contains(&"cluster".to_string()) {
                        light_apis.push("cluster".to_string());
                    }
                    if !light_apis.contains(&"nodes".to_string()) {
                        light_apis.push("nodes".to_string());
                    }
                    light_apis
                } else {
                    vec![]
                }
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

        for req in Self::es_minimum_required() {
            requested.insert(req.to_string());
        }

        let deps = Self::es_dependencies();
        let mut final_set: IndexSet<String> = IndexSet::new();

        fn resolve_deps(
            api: &str,
            deps_map: &HashMap<&'static str, Vec<&'static str>>,
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
            resolve_deps(api, &deps, &mut final_set, &mut visited);
        }

        let mut api_set: IndexSet<ElasticsearchApi> = IndexSet::new();
        for api in final_set.iter() {
            api_set.insert(ElasticsearchApi::parse(api)?);
        }

        let apis: Vec<ElasticsearchApi> = api_set.into_iter().collect();
        Ok(apis)
    }

    pub fn resolve_ls(
        diag_type: &DiagnosticType,
        include: Option<&Vec<String>>,
        exclude: Option<&Vec<String>>,
    ) -> Result<Vec<LogstashApi>> {
        let mut requested: IndexSet<String> = IndexSet::new();

        for api in match diag_type {
            DiagnosticType::Minimal => vec!["node"],
            DiagnosticType::Standard | DiagnosticType::Support | DiagnosticType::Light => {
                vec!["node", "node_stats"]
            }
        } {
            requested.insert(api.to_string());
        }

        if let Some(incs) = include {
            for api in incs {
                LogstashApi::parse(api)?;
                requested.insert(api.to_string());
            }
        }

        if let Some(excs) = exclude {
            for api in excs {
                LogstashApi::parse(api)?;
                requested.swap_remove(api);
            }
        }

        for req in Self::ls_minimum_required() {
            requested.insert(req.to_string());
        }

        let deps = Self::ls_dependencies();
        let mut final_set: IndexSet<String> = IndexSet::new();

        fn resolve_deps(
            api: &str,
            deps_map: &HashMap<&'static str, Vec<&'static str>>,
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
            resolve_deps(api, &deps, &mut final_set, &mut visited);
        }

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
}
