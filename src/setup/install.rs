// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{elasticsearch::*, kibana::*};

use crate::{
    client::Client,
    data::Application,
    embeds::{Assets, KIBANA_ASSETS_BUNDLE},
};
use eyre::{Result, eyre};
use reqwest::{Method, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use url::Url;
use zip::ZipArchive;

// Subdirectory for templates and configs files
pub static ASSETS_FILE: &str = "assets.yml";
pub static SOURCES_FILE: &str = "sources.yml";
pub(super) const KIBANA_ASSETS_DIR: &str = "kibana";

pub(super) struct EmbeddedAssets;

impl EmbeddedAssets {
    pub(super) fn new() -> Result<Self> {
        Ok(Self)
    }

    pub(super) fn get_file(&self, path: &Path) -> Option<std::borrow::Cow<'static, [u8]>> {
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with(KIBANA_ASSETS_DIR) {
                return get_kibana_bundle_file(path_str);
            }
            Assets::get(path_str).map(|f| f.data)
        } else {
            None
        }
    }

    pub(super) fn get_dir_files(&self, path: &Path) -> Vec<(PathBuf, std::borrow::Cow<'static, [u8]>)> {
        let prefix = path.to_str().unwrap_or("");
        if prefix.starts_with(KIBANA_ASSETS_DIR) {
            return get_kibana_bundle_dir_files(prefix);
        }

        let mut files: Vec<_> = Assets::iter()
            .filter(|p| p.starts_with(prefix))
            .filter_map(|p| {
                let p_str = p.as_ref();
                let p_buf = PathBuf::from(p_str);
                Assets::get(p_str).map(|f| (p_buf, f.data))
            })
            .collect();
        files.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
        files
    }
}

pub(super) fn open_kibana_bundle() -> Option<ZipArchive<Cursor<&'static [u8]>>> {
    ZipArchive::new(Cursor::new(KIBANA_ASSETS_BUNDLE)).ok()
}

pub(super) fn get_kibana_bundle_file(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    let mut archive = open_kibana_bundle()?;
    let mut file = archive.by_name(path).ok()?;
    let mut contents = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut contents).ok()?;
    Some(std::borrow::Cow::Owned(contents))
}

pub(super) fn get_kibana_bundle_dir_files(prefix: &str) -> Vec<(PathBuf, std::borrow::Cow<'static, [u8]>)> {
    let Some(mut archive) = open_kibana_bundle() else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut file) = archive.by_index(i) else {
            continue;
        };
        if !file.is_file() || !file.name().starts_with(prefix) {
            continue;
        }

        let mut contents = Vec::with_capacity(file.size() as usize);
        if file.read_to_end(&mut contents).is_ok() {
            files.push((PathBuf::from(file.name()), std::borrow::Cow::Owned(contents)));
        }
    }

    files.sort_by(|(p1, _), (p2, _)| p1.cmp(p2));
    files
}

/// Mapping updates that could not be completed after asset installation.
#[derive(Default)]
pub struct SetupReport {
    pub failed_indices: Vec<String>,
    pub warnings: Vec<String>,
    /// Informational messages, such as a service the source did not declare, that do not make setup partial.
    pub notices: Vec<String>,
}

impl SetupReport {
    pub fn is_complete(&self) -> bool {
        self.failed_indices.is_empty() && self.warnings.is_empty()
    }
}

/// Install assets, requiring complete mapping updates before returning success.
pub async fn assets(client: &Client) -> Result<()> {
    let report = assets_report(client).await?;
    if !report.is_complete() {
        let failed_indices = if report.failed_indices.is_empty() {
            String::new()
        } else {
            format!(
                " These indices could not be updated: {}.",
                report.failed_indices.join(", ")
            )
        };
        let warnings = if report.warnings.is_empty() {
            String::new()
        } else {
            format!(" {}", report.warnings.join(" "))
        };
        return Err(eyre!("Asset installation was partial.{}{}", failed_indices, warnings));
    }
    Ok(())
}

pub async fn assets_report(client: &Client) -> Result<SetupReport> {
    let embedded_assets = EmbeddedAssets::new()?;
    if Application::from(client) == Application::Kibana {
        kibana_assets(client, &embedded_assets, crate::env::get_kibana_space()).await?;
        return Ok(SetupReport::default());
    }
    api_assets(client, &embedded_assets).await
}

/// Install Kibana assets into `space`, where `None` is the default space.
pub async fn kibana_assets_report(client: &Client, space: Option<String>) -> Result<SetupReport> {
    if Application::from(client) != Application::Kibana {
        return Err(eyre!("expected a Kibana client, got {client}"));
    }
    kibana_assets(client, &EmbeddedAssets::new()?, space).await?;
    Ok(SetupReport::default())
}

/// Checks the minimum ESDiag asset set required for processing and Agent Builder.
///
/// This is deliberately read-only and inexpensive enough for onboarding status:
/// the Elasticsearch ingest pipeline proves processing assets are present, while
/// the default Kibana agent must have the ESDiag diagnostic skill attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssetStatus {
    Installed,
    Missing,
    #[default]
    Unknown,
}

pub async fn asset_statuses(
    elasticsearch: &Client,
    kibana: &Client,
    kibana_url: Option<&str>,
) -> (AssetStatus, AssetStatus) {
    let headers = HashMap::new();
    let pipeline = elasticsearch.request(Method::GET, &headers, "_ingest/pipeline/esdiag", None);
    let agent_path = kibana_agent_path(kibana_url);
    let agent = kibana.request(Method::GET, &headers, &agent_path, None);
    let (pipeline, agent) = tokio::join!(pipeline, agent);

    let elasticsearch_assets = match pipeline {
        Ok(response) if response.status().is_success() => AssetStatus::Installed,
        Ok(response) if response.status() == StatusCode::NOT_FOUND => AssetStatus::Missing,
        Ok(_) | Err(_) => AssetStatus::Unknown,
    };
    let kibana_assets = match agent {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(agent) => {
                let installed = agent
                    .get("configuration")
                    .and_then(|configuration| configuration.get("skill_ids"))
                    .and_then(Value::as_array)
                    .is_some_and(|skills| {
                        skills
                            .iter()
                            .any(|skill| skill.as_str() == Some("agentic-diagnostic-assistant"))
                    });
                if installed {
                    AssetStatus::Installed
                } else {
                    AssetStatus::Missing
                }
            }
            Err(_) => AssetStatus::Unknown,
        },
        Ok(response) if response.status() == StatusCode::NOT_FOUND => AssetStatus::Missing,
        Ok(_) | Err(_) => AssetStatus::Unknown,
    };

    (elasticsearch_assets, kibana_assets)
}

pub async fn assets_installed(elasticsearch: &Client, kibana: &Client) -> bool {
    let (elasticsearch_assets, kibana_assets) = asset_statuses(elasticsearch, kibana, None).await;
    elasticsearch_assets == AssetStatus::Installed && kibana_assets == AssetStatus::Installed
}

pub(super) fn kibana_agent_path(kibana_url: Option<&str>) -> String {
    let url = kibana_url
        .map(str::to_string)
        .or_else(|| std::env::var("ESDIAG_KIBANA_URL").ok());
    url.and_then(|url| {
        Url::parse(&url).ok().and_then(|url| {
            let segments = url.path_segments()?.collect::<Vec<_>>();
            segments
                .windows(2)
                .find(|segments| segments[0] == "s")
                .map(|segments| format!("s/{}/api/agent_builder/agents/elastic-ai-agent", segments[1]))
        })
    })
    .unwrap_or_else(|| "/api/agent_builder/agents/elastic-ai-agent".to_string())
}

/// Parses the assets YAML file for the given exporter. Currently only supports Elasticsearch.
pub(super) fn parse_assets_yml(application: Application, assets_store: &EmbeddedAssets) -> Result<Vec<Asset>> {
    let filename = format!("{}/{}", application.key(), ASSETS_FILE);
    let contents = assets_store
        .get_file(Path::new(&filename))
        .ok_or(eyre!("embedded assets did not contain expected file {filename}"))?;
    let assets = yaml_serde::from_slice(&contents)?;
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::super::license::*;
    use super::*;
    use kibana_sync::sync::{SyncBundle, SyncSummary};
    use std::collections::HashSet;

    #[test]
    fn installed_template_version_is_read_from_get_responses() {
        let installed = serde_json::json!({
            "index_templates": [
                { "name": "a-esdiag", "index_template": { "version": 3 } },
                { "name": "b-esdiag", "index_template": {} }
            ]
        });
        assert_eq!(template_version(&installed, "_index_template", "a-esdiag"), Some(3));
        assert_eq!(template_version(&installed, "_index_template", "b-esdiag"), None);
        assert_eq!(template_version(&installed, "_index_template", "c-esdiag"), None);

        let installed = serde_json::json!({
            "component_templates": [{ "name": "esdiag@x", "component_template": { "version": 2 } }]
        });
        assert_eq!(template_version(&installed, "_component_template", "esdiag@x"), Some(2));
    }

    #[test]
    fn only_higher_bundled_versions_trigger_rollover() {
        assert!(template_version_bumped(None, Some(1)));
        assert!(template_version_bumped(Some(1), Some(2)));
        assert!(!template_version_bumped(Some(2), Some(2)));
        assert!(!template_version_bumped(Some(3), Some(2)));
        assert!(!template_version_bumped(None, None));
        assert!(template_version_downgrade(Some(3), Some(2)));
        assert!(template_version_downgrade(Some(1), None));
        assert!(!template_version_downgrade(Some(2), Some(2)));
        assert!(!template_version_downgrade(Some(1), Some(2)));
        assert!(!template_version_downgrade(None, Some(1)));
        assert!(!template_version_downgrade(None, None));
    }

    #[test]
    fn every_bundled_template_has_a_version() {
        let assets = EmbeddedAssets::new().unwrap();
        for dir in ["index_templates", "component_templates"] {
            let files = assets.get_dir_files(&PathBuf::from(format!("elasticsearch/{dir}")));
            assert!(!files.is_empty(), "no bundled {dir}");
            for (path, contents) in files {
                let body: Value = serde_json::from_slice(&contents).unwrap();
                assert!(body["version"].as_u64().is_some(), "{} has no version", path.display());
            }
        }
    }

    #[test]
    fn only_streams_resolving_to_updated_templates_roll_over() {
        let asset = |endpoint: &str| Asset {
            endpoint: endpoint.to_string(),
            method: "PUT".to_string(),
            name: String::new(),
            headers: default_headers(),
            suffix: None,
            query: None,
            requires_security: false,
        };
        let mut changes = TemplateChanges::default();
        changes.record(&asset("_component_template"), "esdiag@ls", b"{}", true);
        changes.record(&asset("_component_template"), "esdiag@mappings", b"{}", false);
        changes.record(
            &asset("_index_template"),
            "ls-esdiag",
            br#"{"composed_of":["esdiag@ls"]}"#,
            false,
        );
        changes.record(
            &asset("_index_template"),
            "a-esdiag",
            br#"{"composed_of":["esdiag@mappings"]}"#,
            true,
        );
        changes.record(
            &asset("_index_template"),
            "b-esdiag",
            br#"{"composed_of":["esdiag@mappings"]}"#,
            false,
        );
        let updated = changes.updated_index_templates();
        assert_eq!(
            updated,
            HashSet::from(["ls-esdiag".to_string(), "a-esdiag".to_string()])
        );

        let data_streams = serde_json::json!({ "data_streams": [
            { "name": "b-esdiag", "template": "b-esdiag" },
            { "name": "a-esdiag", "template": "a-esdiag" },
            { "name": "ls-esdiag", "template": "ls-esdiag" },
            { "name": "no-template-esdiag" },
        ]});
        assert_eq!(
            data_streams_to_roll_over(&data_streams, &updated),
            vec![
                ("a-esdiag".to_string(), "a-esdiag".to_string()),
                ("ls-esdiag".to_string(), "ls-esdiag".to_string()),
            ]
        );
    }

    /// The `diagnostic` properties an index created from the current templates
    /// carries, read from the template itself so the test moves with it.
    fn template_diagnostic_properties() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/elasticsearch/component_templates/esdiag@metadata.json");
        let template: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read metadata template")).expect("parse");
        template["template"]["mappings"]["properties"]["diagnostic"]["properties"].clone()
    }

    /// Which concrete field a provenance name resolves to in one index's mapping.
    fn resolves_to(diagnostic_properties: &Value, field: &str) -> Option<String> {
        let mapping = diagnostic_properties.get(field)?;
        match mapping["type"].as_str()? {
            "alias" => Some(mapping["path"].as_str().expect("alias path").to_string()),
            _ => Some(format!("diagnostic.{field}")),
        }
    }

    /// The mapping of an index created before the provenance rename.
    fn pre_rename_diagnostic_properties() -> Value {
        serde_json::json!({
            "product": { "type": "keyword" },
            "orchestration": { "type": "keyword" },
            "uuid": { "type": "keyword" },
        })
    }

    /// Applies a `_mapping` patch the way Elasticsearch would, so the assertion is
    /// about the mapping the index ends up with rather than the request body.
    fn patched(diagnostic_properties: &Value, patch: &Value) -> Value {
        let mut properties = diagnostic_properties.clone();
        let target = properties.as_object_mut().expect("diagnostic properties");
        let added = patch["properties"]["diagnostic"]["properties"]
            .as_object()
            .expect("patch properties");
        for (field, mapping) in added {
            target.insert(field.clone(), mapping.clone());
        }
        properties
    }

    #[test]
    fn both_provenance_names_resolve_in_either_index_generation() {
        let new_index = template_diagnostic_properties();
        for (current, legacy) in PROVENANCE_RENAMES {
            assert_eq!(new_index[current]["type"], "keyword");
            assert_eq!(new_index[legacy]["type"], "keyword");
        }
        assert!(provenance_mapping_issues(&new_index).is_empty());
        assert!(
            provenance_alias_patch(&new_index).is_none(),
            "a new index needs no patch, so setup is idempotent"
        );

        let old_index = pre_rename_diagnostic_properties();
        assert_eq!(
            resolves_to(&old_index, "application"),
            None,
            "before the patch, the current name resolves to nothing in a historical index"
        );

        let patch = provenance_alias_patch(&old_index).expect("a pre-rename index needs the mirrored aliases");
        let old_index = patched(&old_index, &patch);
        assert_eq!(
            resolves_to(&old_index, "application"),
            resolves_to(&old_index, "product"),
            "after the patch, an old index stores `product` and aliases `application` to it"
        );
        assert_eq!(
            resolves_to(&old_index, "orchestration"),
            resolves_to(&old_index, "platform")
        );
        assert!(
            provenance_alias_patch(&old_index).is_none(),
            "re-running setup over a patched index is a no-op"
        );
    }

    #[test]
    fn provenance_issues_identify_aliases_and_unlinked_fields() {
        let old = pre_rename_diagnostic_properties();
        assert!(provenance_mapping_issues(&old).is_empty());
        let patch = provenance_alias_patch(&old).unwrap();
        assert_eq!(
            provenance_mapping_issues(&patched(&old, &patch)),
            vec![
                ProvenanceIssue::QueryOnlyAlias("application"),
                ProvenanceIssue::QueryOnlyAlias("platform"),
            ]
        );
        let split = serde_json::json!({
            "application": {"type": "keyword"},
            "product": {"type": "keyword"}
        });
        let issues = provenance_mapping_issues(&split);
        assert!(issues[0].to_string().contains("unlinked concrete fields"));
        assert!(provenance_alias_patch(&split).is_none());
        let alias = serde_json::json!({
            "application": {"type": "keyword"},
            "product": {"type": "alias", "path": "diagnostic.application"}
        });
        assert!(
            provenance_mapping_issues(&alias)[0]
                .to_string()
                .contains("diagnostic.product is a query-only alias")
        );
        assert!(provenance_mapping_issues(&Value::Null).is_empty());
    }

    #[test]
    fn only_backing_indices_behind_the_write_index_are_historical() {
        let data_streams = serde_json::json!({ "data_streams": [
            { "name": "metrics-task-esdiag", "indices": [
                { "index_name": ".ds-metrics-task-esdiag-2026.09.29-000223" },
                { "index_name": ".ds-metrics-task-esdiag-2026.10.13-000231" },
                { "index_name": ".ds-metrics-task-esdiag-2026.10.14-000232" },
            ]},
            { "name": "metrics-node-esdiag", "indices": [
                { "index_name": ".ds-metrics-node-esdiag-2026.10.14-000001" },
            ]},
            { "name": "empty-esdiag", "indices": [] },
        ]});
        let historical = historical_backing_indices(&data_streams);
        assert_eq!(
            historical,
            HashMap::from([
                (
                    ".ds-metrics-task-esdiag-2026.09.29-000223".to_string(),
                    "metrics-task-esdiag".to_string()
                ),
                (
                    ".ds-metrics-task-esdiag-2026.10.13-000231".to_string(),
                    "metrics-task-esdiag".to_string()
                ),
            ])
        );
        assert!(historical_backing_indices(&Value::Null).is_empty());
    }

    #[test]
    fn historical_provenance_warnings_collapse_to_one_line_per_data_stream() {
        let index = |diagnostic: Value| serde_json::json!({ "mappings": { "properties": { "diagnostic": { "properties": diagnostic } } } });
        let unlinked = serde_json::json!({
            "application": {"type": "keyword"},
            "product": {"type": "keyword"},
            "platform": {"type": "keyword"},
            "orchestration": {"type": "keyword"},
        });
        let aliased = serde_json::json!({
            "product": {"type": "keyword"},
            "application": {"type": "alias", "path": "diagnostic.product"},
        });
        let mappings = serde_json::json!({
            ".ds-metrics-diagnostic-esdiag-2025.07.28-000004": index(aliased.clone()),
            ".ds-metrics-diagnostic-esdiag-2026.07.29-000023": index(unlinked.clone()),
            ".ds-metrics-diagnostic-esdiag-2026.09.27-000028": index(unlinked.clone()),
            ".ds-metrics-diagnostic-esdiag-2026.10.14-000029": index(aliased),
            ".ds-settings-ilm-esdiag-2026.10.12-000135": index(serde_json::json!({
                "platform": {"type": "keyword"},
                "orchestration": {"type": "keyword"},
            })),
            ".ds-metrics-node-esdiag-2026.10.14-000133": index(template_diagnostic_properties()),
        });
        let historical = HashMap::from([
            (
                ".ds-metrics-diagnostic-esdiag-2025.07.28-000004".to_string(),
                "metrics-diagnostic-esdiag".to_string(),
            ),
            (
                ".ds-metrics-diagnostic-esdiag-2026.07.29-000023".to_string(),
                "metrics-diagnostic-esdiag".to_string(),
            ),
            (
                ".ds-metrics-diagnostic-esdiag-2026.09.27-000028".to_string(),
                "metrics-diagnostic-esdiag".to_string(),
            ),
            (
                ".ds-settings-ilm-esdiag-2026.10.12-000135".to_string(),
                "settings-ilm-esdiag".to_string(),
            ),
        ]);

        let warnings = provenance_mapping_warnings(&mappings, &historical);

        assert_eq!(warnings.len(), 3, "{warnings:#?}");
        assert!(
            warnings[0].starts_with(
                ".ds-metrics-diagnostic-esdiag-2026.10.14-000029: diagnostic.application is a query-only alias"
            ),
            "the write index keeps its per-index warning: {}",
            warnings[0]
        );
        assert!(warnings[1].starts_with(
            "metrics-diagnostic-esdiag: 2 older backing indices store diagnostic.application and diagnostic.product; \
             diagnostic.platform and diagnostic.orchestration as unlinked concrete fields"
        ));
        assert!(warnings[2].starts_with(
            "settings-ilm-esdiag: 1 older backing index stores diagnostic.platform and diagnostic.orchestration"
        ));
        assert!(
            !warnings.iter().any(|warning| warning.contains("2025.07.28")),
            "historical aliases never see a writer"
        );
    }

    #[test]
    fn provenance_alias_patch_skips_mappings_with_nothing_to_bridge() {
        assert!(provenance_alias_patch(&Value::Null).is_none());
        assert!(provenance_alias_patch(&serde_json::json!({ "uuid": { "type": "keyword" } })).is_none());
        assert!(
            provenance_alias_patch(&serde_json::json!({
                "product": { "type": "alias", "path": "diagnostic.application" },
                "application": { "type": "keyword" },
            }))
            .is_none()
        );
    }

    #[test]
    fn provenance_alias_patches_select_only_the_pre_rename_indices() {
        let mappings = serde_json::json!({
            ".ds-metrics-node-esdiag-2024.01.01-000001": {
                "mappings": { "properties": { "diagnostic": { "properties": pre_rename_diagnostic_properties() } } }
            },
            ".ds-metrics-node-esdiag-2026.07.01-000002": {
                "mappings": { "properties": { "diagnostic": { "properties": template_diagnostic_properties() } } }
            },
            "unrelated-esdiag-lookalike": { "mappings": { "properties": { "message": { "type": "text" } } } },
        });

        let patches = provenance_alias_patches(&mappings);

        assert_eq!(patches.len(), 1, "only the historical backing index is patched");
        let (index, patch) = &patches[0];
        assert_eq!(index, ".ds-metrics-node-esdiag-2024.01.01-000001");
        assert_eq!(
            patch["properties"]["diagnostic"]["properties"]["application"],
            serde_json::json!({ "type": "alias", "path": "diagnostic.product" })
        );
    }

    #[test]
    fn test_asset_deserialization_with_requires_security() {
        let yaml = r#"
- name: "roles"
  endpoint: "_security/role"
  method: "PUT"
  requires_security: true
- name: "ingest_pipelines"
  endpoint: "_ingest/pipeline"
  method: "PUT"
"#;
        let assets: Vec<Asset> = yaml_serde::from_str(yaml).unwrap();
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].name, "roles");
        assert!(assets[0].requires_security);
        assert_eq!(assets[1].name, "ingest_pipelines");
        assert!(!assets[1].requires_security);
    }

    #[test]
    fn test_should_skip_asset() {
        let security_asset = Asset {
            endpoint: "/".to_string(),
            method: "GET".to_string(),
            name: "test".to_string(),
            headers: HashMap::new(),
            suffix: None,
            query: None,
            requires_security: true,
        };
        let normal_asset = Asset {
            endpoint: "/".to_string(),
            method: "GET".to_string(),
            name: "test".to_string(),
            headers: HashMap::new(),
            suffix: None,
            query: None,
            requires_security: false,
        };

        // Security enabled: skip nothing
        assert!(!should_skip_asset(&security_asset, true));
        assert!(!should_skip_asset(&normal_asset, true));

        // Security disabled: skip security asset
        assert!(should_skip_asset(&security_asset, false));
        assert!(!should_skip_asset(&normal_asset, false));
    }

    #[test]
    fn existing_esdiag_dashboards_are_found_by_id_or_origin_id() {
        let ids = HashSet::from(["esdiag-readme".to_string()]);
        let found = |objects: Value| contains_esdiag_saved_object(&serde_json::json!({"saved_objects": objects}), &ids);

        assert!(found(serde_json::json!([{"id": "esdiag-readme"}])));
        assert!(found(
            serde_json::json!([{"id": "a1b2c3", "originId": "esdiag-readme"}])
        ));
        assert!(!found(serde_json::json!([{"id": "my-dashboard"}])));
        assert!(!found(serde_json::json!([])));
    }

    #[test]
    fn kibana_space_paths_omit_the_default_space() {
        assert_eq!(kibana_space_path("default", "api/x"), "api/x");
        assert_eq!(kibana_space_path("ops team", "api/x"), "s/ops%20team/api/x");
    }

    #[test]
    fn kibana_assets_follow_kibana_sync_bundle_layout() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        assert_eq!(bundle.spaces.len(), 1);
        assert_eq!(bundle.spaces[0]["id"], "esdiag");

        let esdiag = bundle.by_space.get("esdiag").unwrap();
        assert_eq!(esdiag.saved_objects.len(), 95);
        assert_eq!(esdiag.workflows.len(), 1);
        assert!(esdiag.agents.is_empty());
        assert_eq!(esdiag.tools.len(), 1);
        assert_eq!(esdiag.skills.len(), 1);
        assert!(esdiag.skills[0]["referenced_content"].as_array().unwrap().len() > 1);
    }

    #[test]
    fn agent_builder_license_requires_an_active_trial_or_enterprise_license() {
        assert!(agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "trial"}
        })));
        assert!(agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "enterprise"}
        })));
        assert!(!agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "active", "type": "basic"}
        })));
        assert!(!agent_builder_license_is_active(&serde_json::json!({
            "license": {"status": "expired", "type": "trial"}
        })));
    }

    #[test]
    fn kibana_sync_phases_keep_saved_objects_license_independent() {
        let mut bundle = SyncBundle::default();
        bundle
            .spaces
            .push(serde_json::json!({"id": "esdiag", "name": "esdiag"}));
        bundle.by_space.insert(
            "esdiag".to_string(),
            kibana_sync::sync::SpaceBundle {
                saved_objects: vec![serde_json::json!({"id": "dashboard-1"})],
                workflows: vec![serde_json::json!({"id": "workflow-1"})],
                tools: vec![serde_json::json!({"id": "tool-1"})],
                skills: vec![serde_json::json!({"id": "skill-1"})],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );

        let saved_objects = saved_objects_bundle(&bundle);
        let agent_builder = agent_builder_bundle(bundle);

        assert_eq!(saved_objects.spaces.len(), 1);
        assert_eq!(saved_objects.by_space["esdiag"].saved_objects.len(), 1);
        assert!(saved_objects.by_space["esdiag"].tools.is_empty());
        assert!(agent_builder.spaces.is_empty());
        assert!(agent_builder.by_space["esdiag"].saved_objects.is_empty());
        assert_eq!(agent_builder.by_space["esdiag"].workflows.len(), 1);
        assert_eq!(agent_builder.by_space["esdiag"].tools.len(), 1);
        assert_eq!(agent_builder.by_space["esdiag"].skills.len(), 1);

        let workflows = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Workflows);
        let tools = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Tools);
        let skills = kibana_asset_bundle(&agent_builder, KibanaAssetKind::Skills);
        assert_eq!(workflows.by_space["esdiag"].workflows.len(), 1);
        assert!(workflows.by_space["esdiag"].tools.is_empty());
        assert_eq!(tools.by_space["esdiag"].tools.len(), 1);
        assert!(tools.by_space["esdiag"].skills.is_empty());
        assert_eq!(skills.by_space["esdiag"].skills.len(), 1);
        assert!(skills.by_space["esdiag"].tools.is_empty());
    }

    #[test]
    fn incomplete_kibana_sync_prevents_default_agent_updates() {
        let mut bundle = SyncBundle::default();
        bundle.by_space.insert(
            "esdiag".to_string(),
            kibana_sync::sync::SpaceBundle {
                saved_objects: vec![serde_json::json!({"id": "object-1"})],
                workflows: vec![serde_json::json!({"id": "workflow-1"})],
                skills: vec![serde_json::json!({"id": "skill-1"})],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );

        let mut summary = SyncSummary::default();
        accumulate_sync_summary(
            &mut summary,
            SyncSummary {
                saved_objects_attempted: 1,
                saved_objects_applied: 1,
                ..SyncSummary::default()
            },
        );
        accumulate_sync_summary(
            &mut summary,
            SyncSummary {
                workflows_attempted: 1,
                ..SyncSummary::default()
            },
        );

        let error = ensure_sync_completed(&bundle, &summary).unwrap_err();
        assert!(error.to_string().contains("saved objects 1/1"));
        assert!(error.to_string().contains("workflows 0/1"));
        assert!(error.to_string().contains("skills 0/1"));
    }

    #[test]
    fn sync_error_preserves_counts_from_completed_phases() {
        let mut bundle = SyncBundle::default();
        bundle.by_space.insert(
            "default".to_string(),
            kibana_sync::sync::SpaceBundle {
                saved_objects: vec![serde_json::json!({"id": "object-1"})],
                workflows: vec![serde_json::json!({"id": "workflow-1"})],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );
        let mut summary = SyncSummary {
            saved_objects_attempted: 1,
            saved_objects_applied: 1,
            ..SyncSummary::default()
        };

        let error = accumulate_sync_result(
            &bundle,
            &mut summary,
            Err::<SyncSummary, _>(std::io::Error::other("workflow request failed")),
        )
        .unwrap_err();

        assert!(error.to_string().contains("saved objects 1/1"));
        assert!(error.to_string().contains("workflows 0/1"));
        assert_eq!(
            error.downcast_ref::<std::io::Error>().map(std::io::Error::kind),
            Some(std::io::ErrorKind::Other)
        );
    }

    #[tokio::test]
    async fn repeated_kibana_sync_updates_existing_workflow() {
        use kibana_sync::test_support::{MockResponse, TestServer};

        let workflow = serde_json::json!({
            "id": "workflow-1",
            "name": "Diagnostic workflow",
            "definition": {"name": "desired"}
        });
        let item_path = "/api/workflows/workflow/workflow-1";
        let server = TestServer::new(vec![
            MockResponse {
                method: "GET",
                path: "/api/status",
                status: 200,
                body: serde_json::json!({"version": {"number": "9.4.2"}}),
            },
            MockResponse {
                method: "GET",
                path: item_path,
                status: 404,
                body: serde_json::json!({}),
            },
            MockResponse {
                method: "POST",
                path: "/api/workflows/workflow",
                status: 200,
                body: serde_json::json!({}),
            },
            MockResponse {
                method: "GET",
                path: item_path,
                status: 200,
                body: serde_json::json!({
                    "id": "workflow-1",
                    "readonly": false,
                    "definition": {"name": "existing"}
                }),
            },
            MockResponse {
                method: "PUT",
                path: item_path,
                status: 200,
                body: serde_json::json!({}),
            },
        ]);
        let client = server.client().unwrap();
        let mut bundle = SyncBundle::default();
        bundle.by_space.insert(
            "default".to_string(),
            kibana_sync::sync::SpaceBundle {
                workflows: vec![workflow.clone()],
                ..kibana_sync::sync::SpaceBundle::default()
            },
        );

        sync_kibana_bundle(&client, &bundle).await.unwrap();
        sync_kibana_bundle(&client, &bundle).await.unwrap();

        let requests = server.requests();
        assert!(requests.iter().all(|request| request.method != "HEAD"));
        assert_eq!(
            requests
                .iter()
                .map(|request| (request.method.as_str(), request.path.as_str()))
                .collect::<Vec<_>>(),
            [
                ("GET", "/api/status"),
                ("GET", item_path),
                ("POST", "/api/workflows/workflow"),
                ("GET", item_path),
                ("PUT", item_path),
            ]
        );
        let update = requests.iter().find(|request| request.method == "PUT").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&update.body).unwrap(), workflow);
    }

    #[test]
    fn kibana_assets_are_embedded_as_bundle_not_raw_files() {
        assert!(!KIBANA_ASSETS_BUNDLE.is_empty());
        assert!(Assets::get("kibana/spaces.yml").is_none());

        let embedded_assets = EmbeddedAssets::new().unwrap();
        let spaces = embedded_assets
            .get_file(Path::new("kibana/spaces.yml"))
            .expect("Kibana spaces manifest should load from bundle");

        assert!(std::str::from_utf8(&spaces).unwrap().contains("id: esdiag"));
    }

    #[test]
    fn kibana_bundle_uses_the_spaces_manifest() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        assert_eq!(bundle.spaces.len(), 1);
        assert_eq!(bundle.spaces[0]["id"], "esdiag");
        assert_eq!(bundle.spaces[0]["name"], "esdiag");
        assert_eq!(bundle.spaces[0]["description"], "Elastic Stack Diagnostics");
        assert_eq!(bundle.spaces[0]["solution"], "oblt");
    }

    #[test]
    fn kibana_sync_resolves_saved_objects_from_display_name_files() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();
        let saved_objects = &bundle.by_space["esdiag"].saved_objects;

        assert_eq!(saved_objects.len(), 95);
        assert_eq!(saved_objects[0]["type"], "dashboard");
        assert_eq!(saved_objects[0]["id"], "alias-detail");
    }

    #[test]
    fn kibana_sync_preserves_saved_object_json_string_fields() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();

        for object in &bundle.by_space["esdiag"].saved_objects {
            let label = saved_object_label(object);
            let attributes = object
                .get("attributes")
                .unwrap_or_else(|| panic!("{label} should have attributes"));

            assert_json_string_fields_parse(&label, attributes);
            assert_vega_spec_parses(&label, attributes);
        }
    }

    #[test]
    fn kibana_readme_dashboard_links_to_esdiag_issues() {
        let embedded_assets = EmbeddedAssets::new().unwrap();
        let bundle = kibana_bundle(&embedded_assets).unwrap().read_all().unwrap();
        let readme = bundle.by_space["esdiag"]
            .saved_objects
            .iter()
            .find(|object| object["id"] == "esdiag-readme")
            .expect("readme dashboard should be embedded");
        let content = serde_json::to_string(readme).unwrap();

        assert!(content.contains("https://github.com/elastic/esdiag/issues"));
        assert!(!content.contains("https://github.com/elastic/issues)"));
    }

    fn saved_object_label(object: &Value) -> String {
        format!(
            "{}/{}",
            object["type"].as_str().unwrap_or("<missing-type>"),
            object["id"].as_str().unwrap_or("<missing-id>")
        )
    }

    fn assert_json_string_fields_parse(label: &str, value: &Value) {
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    if let Some(text) = child.as_str()
                        && (key == "visState" || key.ends_with("JSON"))
                    {
                        serde_json::from_str::<Value>(text)
                            .unwrap_or_else(|err| panic!("{label}.{key} should parse as JSON: {err}"));
                    }
                    assert_json_string_fields_parse(label, child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    assert_json_string_fields_parse(label, child);
                }
            }
            _ => {}
        }
    }

    fn assert_vega_spec_parses(label: &str, attributes: &Value) {
        let Some(vis_state) = attributes.get("visState").and_then(Value::as_str) else {
            return;
        };
        let vis_state: Value = serde_json::from_str(vis_state)
            .unwrap_or_else(|err| panic!("{label}.visState should parse as JSON: {err}"));

        if vis_state["type"].as_str() != Some("vega") {
            return;
        }

        let spec = vis_state["params"]["spec"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}.visState.params.spec should be a string"));
        serde_json::from_str::<Value>(spec)
            .unwrap_or_else(|err| panic!("{label}.visState.params.spec should parse as JSON: {err}"));
    }
}
