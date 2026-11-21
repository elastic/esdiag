// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{ElasticContextTarget, KnownHost, ResolvedKnownHost};
use eyre::{Result, eyre};
use std::fmt::{Display, Formatter};

/// A name that selects a host wherever ESDiag accepts one.
///
/// A reference names a saved host, a template host (`<template>://<id>[/<product>]`),
/// or an Elastic CLI context service (`.service`, `.context.service`, or
/// `.context.cloud/<deployment>/es`). Every source resolves to the same
/// [`ResolvedKnownHost`], so callers never branch on where a host came from.
///
/// Elastic CLI context references take precedence over saved host names, and
/// [`KnownHost::save`] rejects names that would be read as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostReference {
    name: String,
    source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    Saved,
    Template,
    ElasticContext(ElasticContextTarget),
}

impl HostReference {
    /// Classify `value` as a host reference without resolving credentials.
    ///
    /// Returns `Ok(None)` when `value` names no host, so callers can treat it as
    /// a URL or path. A recognized but malformed reference is an error.
    pub fn parse(value: &str) -> Result<Option<Self>> {
        let name = value.trim();
        let source = if let Some(target) = ElasticContextTarget::parse(name)? {
            Source::ElasticContext(target)
        } else if KnownHost::is_template_reference(name)? {
            Source::Template
        } else if KnownHost::get_known(&name.to_string()).is_some() {
            Source::Saved
        } else {
            return Ok(None);
        };
        Ok(Some(Self {
            name: name.to_string(),
            source,
        }))
    }

    /// Whether `name` would be read as an Elastic CLI context reference.
    pub(crate) fn is_reserved_name(name: &str) -> bool {
        !matches!(ElasticContextTarget::parse(name.trim()), Ok(None))
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Resolve this reference into a concrete host for runtime dispatch.
    pub fn resolve(&self) -> Result<ResolvedKnownHost> {
        let host = match &self.source {
            Source::Saved => {
                let host = KnownHost::get_known(&self.name)
                    .ok_or_else(|| eyre!("Host '{}' is no longer configured", self.name))?;
                if host.is_template() {
                    return Err(eyre!(KnownHost::template_guidance(&self.name)));
                }
                host
            }
            Source::Template => KnownHost::resolve_template_reference(&self.name)?
                .ok_or_else(|| eyre!("Template reference '{}' did not resolve", self.name))?,
            Source::ElasticContext(target) => target.resolve_host()?,
        };
        host.resolve()
    }
}

impl Display for HostReference {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::HostReference;
    use crate::data::{Application, Auth, HostRole, HostRoute, KnownHost};
    use std::collections::BTreeMap;
    use url::Url;

    fn write_config(env: &mut crate::TestEnv, contents: &str) {
        let config_file = env.tmp.path().join("elasticrc.yml");
        std::fs::write(&config_file, contents).expect("write config");
        env.set_path("ELASTIC_CLI_CONFIG_FILE", config_file);
    }

    fn save_host(name: &str, url: &str) {
        let mut hosts = BTreeMap::new();
        hosts.insert(
            name.to_string(),
            KnownHost::new_no_auth(
                Application::Elasticsearch,
                Url::parse(url).expect("url"),
                vec![HostRole::Collect, HostRole::Send],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
    }

    #[test]
    fn urls_paths_and_unknown_names_are_not_references() {
        let _env = crate::TestEnv::new();

        for value in [
            "https://es.example:9200",
            "./diagnostic.zip",
            "missing-host",
            "./.es",
            "../out",
            "-",
        ] {
            assert_eq!(HostReference::parse(value).expect("parse"), None, "{value}");
        }
    }

    #[test]
    fn saved_host_reference_resolves_from_hosts_yml() {
        let _env = crate::TestEnv::new();
        save_host("prod", "https://prod.example:9200");

        let reference = HostReference::parse(" prod ").expect("parse").expect("reference");
        let resolved = reference.resolve().expect("resolve");

        assert_eq!(reference.as_str(), "prod");
        assert_eq!(
            resolved.as_ref().concrete_url().map(Url::as_str),
            Some("https://prod.example:9200/")
        );
    }

    #[test]
    fn context_references_parse_without_configuration() {
        let _env = crate::TestEnv::new();

        let reference = HostReference::parse(".prod.es").expect("parse").expect("reference");

        assert_eq!(reference.to_string(), ".prod.es");
    }

    #[test]
    fn context_reference_names_are_reserved() {
        assert!(HostReference::is_reserved_name(".prod.es"));
        assert!(HostReference::is_reserved_name(".kb"));
        assert!(HostReference::is_reserved_name(".cloud"));
        assert!(!HostReference::is_reserved_name(".prod.ls"));
        assert!(!HostReference::is_reserved_name("prod"));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn context_reference_takes_precedence_over_a_saved_host_of_the_same_name() {
        let mut env = crate::TestEnv::new();
        std::fs::write(
            &env.hosts_path,
            ".prod.es:\n  app: elasticsearch\n  url: https://saved.example:9200\n  roles: [collect, send]\n",
        )
        .expect("write pre-existing hosts.yml");
        assert!(KnownHost::get_known(&".prod.es".to_string()).is_some());
        write_config(
            &mut env,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://context.example:9200\n",
        );

        let resolved = HostReference::parse(".prod.es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve");

        assert_eq!(
            resolved.as_ref().concrete_url().map(Url::as_str),
            Some("https://context.example:9200/")
        );
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn context_services_resolve_with_the_roles_their_application_supports() {
        let mut env = crate::TestEnv::new();
        write_config(
            &mut env,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://prod.example:9200\n      auth:\n        api_key: es-key\n    kibana:\n      url: https://prod.example:5601\n",
        );

        let elasticsearch = HostReference::parse(".prod.es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve es");
        let kibana = HostReference::parse(".prod.kb")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve kb");

        assert_eq!(elasticsearch.application(), Application::Elasticsearch);
        assert_eq!(elasticsearch.as_ref().roles(), [HostRole::Collect, HostRole::Send]);
        assert!(matches!(
            elasticsearch.as_ref().get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "es-key"
        ));
        assert_eq!(kibana.application(), Application::Kibana);
        assert_eq!(kibana.as_ref().roles(), [HostRole::Collect, HostRole::View]);
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn active_context_reference_uses_input_credentials_only() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_ELASTIC_CLI", "1");
        env.set("ELASTIC_ES_URL", "https://active.example:9200");
        env.set("ELASTIC_ES_API_KEY", "input-key");
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "output-key");

        let resolved = HostReference::parse(".es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve");

        assert_eq!(
            resolved.as_ref().concrete_url().map(Url::as_str),
            Some("https://active.example:9200/")
        );
        assert!(matches!(
            resolved.as_ref().get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "input-key"
        ));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_context_reference_reports_missing_context() {
        let mut env = crate::TestEnv::new();
        write_config(
            &mut env,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://prod.example:9200\n",
        );

        let error = HostReference::parse(".diag.es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect_err("missing context must fail");

        assert!(error.to_string().contains("Elastic CLI context 'diag' was not found"));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn context_elasticsearch_reference_carries_its_kibana_viewer() {
        let mut env = crate::TestEnv::new();
        env.remove("MISSING_KIBANA_KEY");
        write_config(
            &mut env,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://prod.example:9200\n    kibana:\n      url: https://prod.example:5601\n  bare:\n    elasticsearch:\n      url: https://bare.example:9200\n  broken:\n    elasticsearch:\n      url: https://broken.example:9200\n    kibana:\n      url: https://broken.example:5601\n      auth:\n        api_key: $(env:MISSING_KIBANA_KEY)\n",
        );
        let resolve = |reference: &str| {
            HostReference::parse(reference)
                .expect("parse")
                .expect("reference")
                .resolve()
                .expect("resolve")
                .into_known_host()
        };

        let prod = resolve(".prod.es");
        let viewer = prod.attached_viewer().expect("prod viewer");
        assert_eq!(viewer.app(), Some(Application::Kibana));
        assert_eq!(
            viewer.concrete_url().map(Url::as_str),
            Some("https://prod.example:5601/")
        );

        assert!(resolve(".bare.es").attached_viewer().is_none());
        assert!(
            resolve(".broken.es").attached_viewer().is_none(),
            "an unresolvable Kibana service omits the link instead of failing the export"
        );
        assert!(resolve(".prod.kb").attached_viewer().is_none());
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn context_reference_follows_configuration_changes() {
        let mut env = crate::TestEnv::new();
        let config = |url: &str, key: &str| {
            format!(
                "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: {url}\n      auth:\n        api_key: {key}\n"
            )
        };
        write_config(&mut env, &config("https://one.example:9200", "first-key"));
        let reference = HostReference::parse(".prod.es").expect("parse").expect("reference");
        let first = reference.resolve().expect("first resolution");
        assert_eq!(
            first.as_ref().concrete_url().map(Url::as_str),
            Some("https://one.example:9200/")
        );

        write_config(&mut env, &config("https://two.example:9200", "other-key"));
        let second = reference.resolve().expect("reloaded resolution");

        assert_eq!(
            second.as_ref().concrete_url().map(Url::as_str),
            Some("https://two.example:9200/")
        );
        assert!(matches!(
            second.as_ref().get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "other-key"
        ));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_cloud_reference_materializes_admin_proxy_route() {
        let mut env = crate::TestEnv::new();
        write_config(
            &mut env,
            "current_context: prod\ncontexts:\n  prod:\n    cloud:\n      url: https://api.elastic-cloud.com\n      auth:\n        api_key: cloud-key\n",
        );

        let resolved = HostReference::parse(".prod.cloud/deployment-123/es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve cloud reference");

        assert_eq!(resolved.application(), Application::Elasticsearch);
        assert_eq!(resolved.route(), HostRoute::ElasticCloudAdmin);
        assert_eq!(
            resolved.as_ref().concrete_url().map(Url::as_str),
            Some("https://api.elastic-cloud.com/api/v1/deployments/deployment-123/elasticsearch/_main/proxy/")
        );
        assert_eq!(resolved.as_ref().roles(), [HostRole::Collect]);
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn active_cloud_reference_materializes_admin_proxy_route() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_ELASTIC_CLI", "1");
        env.set("ELASTIC_CLOUD_URL", "https://api.elastic-cloud.com");
        env.set("ELASTIC_CLOUD_API_KEY", "active-cloud-key");

        let resolved = HostReference::parse(".cloud/deployment-456/es")
            .expect("parse")
            .expect("reference")
            .resolve()
            .expect("resolve cloud reference");

        assert_eq!(resolved.route(), HostRoute::ElasticCloudAdmin);
        assert_eq!(
            resolved.as_ref().concrete_url().map(Url::as_str),
            Some("https://api.elastic-cloud.com/api/v1/deployments/deployment-456/elasticsearch/_main/proxy/")
        );
        assert!(matches!(
            resolved.as_ref().get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "active-cloud-key"
        ));
    }
}
