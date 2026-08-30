// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    Application, ApplicationConfig, Auth, CredentialDirection, ElasticContextTarget, ElasticOutputContext, HostRole,
    KnownHost, KnownHostBuilder,
};
use eyre::{Result, eyre};
use std::env;
use url::Url;

/// The source selected by output-deployment precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputDeploymentSource {
    Explicit,
    ElasticContext,
    Environment,
    Persisted,
}

/// One Elasticsearch output target and its attached Kibana viewer.
///
/// The resolver selects every endpoint and credential from one source. It never
/// fills an incomplete environment declaration from persisted state.
#[derive(Clone, Debug)]
pub struct OutputDeployment {
    pub source: OutputDeploymentSource,
    pub elasticsearch: KnownHost,
    pub elasticsearch_auth: Auth,
    pub kibana: Option<KnownHost>,
    pub kibana_auth: Option<Auth>,
}

impl OutputDeployment {
    pub fn resolve(explicit_target: Option<&str>, require_kibana: bool) -> Result<Self> {
        if let Some(target) = explicit_target {
            return Self::from_explicit(target, require_kibana);
        }
        let config = ApplicationConfig::load()?;
        if let Some(context) = config.output.elastic_context {
            return Self::from_elastic_context(&context, require_kibana);
        }
        if runtime_output_is_declared() {
            return Self::from_environment(require_kibana);
        }
        Self::from_persisted(config, require_kibana)
    }

    fn from_explicit(target: &str, require_kibana: bool) -> Result<Self> {
        if let Some(target) = ElasticContextTarget::parse(target)? {
            let (elasticsearch, elasticsearch_auth, kibana, kibana_auth) =
                target.resolve_output_hosts(require_kibana)?;
            return Ok(Self {
                source: OutputDeploymentSource::Explicit,
                elasticsearch,
                elasticsearch_auth,
                kibana,
                kibana_auth,
            });
        }
        let output = KnownHost::get_known(&target.to_string())
            .ok_or_else(|| eyre!("Explicit output deployment target '{target}' must be a saved Elasticsearch host"))?;
        Self::from_saved(OutputDeploymentSource::Explicit, output, require_kibana)
    }

    fn from_persisted(config: ApplicationConfig, require_kibana: bool) -> Result<Self> {
        let output_name = config.output.default.ok_or_else(|| {
            eyre!("No output deployment is configured. Run `esdiag init` or provide an explicit output target.")
        })?;
        let output = KnownHost::get_known(&output_name)
            .ok_or_else(|| eyre!("Configured output host '{output_name}' was not found"))?;
        Self::from_saved(OutputDeploymentSource::Persisted, output, require_kibana)
    }

    pub fn from_elastic_context(context: &ElasticOutputContext, require_kibana: bool) -> Result<Self> {
        let target = ElasticContextTarget {
            context: Some(context.context.clone()),
            application: Application::Elasticsearch,
            cloud_deployment: None,
            config_file: context.config_file.clone(),
        };
        let (elasticsearch, elasticsearch_auth, kibana, kibana_auth) = target.resolve_output_hosts(require_kibana)?;
        Ok(Self {
            source: OutputDeploymentSource::ElasticContext,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        })
    }

    pub fn from_elastic_target(target: &ElasticContextTarget, require_kibana: bool) -> Result<Self> {
        let (elasticsearch, elasticsearch_auth, kibana, kibana_auth) = target.resolve_output_hosts(require_kibana)?;
        Ok(Self {
            source: OutputDeploymentSource::Explicit,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        })
    }

    fn from_saved(source: OutputDeploymentSource, elasticsearch: KnownHost, require_kibana: bool) -> Result<Self> {
        validate_send_host(&elasticsearch, "output deployment")?;
        let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;

        let (kibana, kibana_auth) = if require_kibana {
            let viewer_name = elasticsearch
                .viewer()
                .ok_or_else(|| eyre!("Output deployment requires a linked Kibana viewer"))?;
            let viewer = KnownHost::get_known(&viewer_name.to_string())
                .ok_or_else(|| eyre!("Output deployment viewer '{viewer_name}' was not found"))?;
            validate_view_host(&viewer, viewer_name)?;
            let auth = viewer.get_auth_for_direction(CredentialDirection::Output)?;
            (Some(viewer), Some(auth))
        } else {
            (None, None)
        };

        Ok(Self {
            source,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        })
    }

    fn from_environment(require_kibana: bool) -> Result<Self> {
        let output_url = required_environment("ESDIAG_OUTPUT_URL")?;
        let (apikey, username, password) = environment_auth()?;
        let elasticsearch = KnownHostBuilder::new(Url::parse(&output_url)?)
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .apikey(apikey.clone())
            .username(username.clone())
            .password(password.clone())
            .build()?;
        let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;

        let (kibana, kibana_auth) = if require_kibana {
            let kibana_url = required_environment("ESDIAG_KIBANA_URL")?;
            let kibana = KnownHostBuilder::new(Url::parse(&kibana_url)?)
                .application(Application::Kibana)
                .roles(vec![HostRole::View])
                .apikey(apikey)
                .username(username)
                .password(password)
                .build()?;
            let auth = kibana.get_auth_for_direction(CredentialDirection::Output)?;
            (Some(kibana), Some(auth))
        } else {
            (None, None)
        };

        Ok(Self {
            source: OutputDeploymentSource::Environment,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        })
    }
}

fn runtime_output_is_declared() -> bool {
    env::var_os("ESDIAG_OUTPUT_URL").is_some()
}

fn required_environment(name: &str) -> Result<String> {
    let value = env::var(name).map_err(|_| eyre!("{name} is not defined"))?;
    if value.trim().is_empty() {
        return Err(eyre!("{name} is empty"));
    }
    Ok(value)
}

fn environment_auth() -> Result<(Option<String>, Option<String>, Option<String>)> {
    let apikey = env::var("ESDIAG_OUTPUT_APIKEY")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let username = env::var("ESDIAG_OUTPUT_USERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let password = env::var("ESDIAG_OUTPUT_PASSWORD")
        .ok()
        .filter(|value| !value.trim().is_empty());

    match (&apikey, &username, &password) {
        (Some(_), None, None) | (None, None, None) | (None, Some(_), Some(_)) => Ok((apikey, username, password)),
        (Some(_), _, _) => Err(eyre!(
            "ESDIAG_OUTPUT_APIKEY cannot be combined with ESDIAG_OUTPUT_USERNAME or ESDIAG_OUTPUT_PASSWORD"
        )),
        (None, Some(_), None) | (None, None, Some(_)) => Err(eyre!(
            "ESDIAG_OUTPUT_USERNAME and ESDIAG_OUTPUT_PASSWORD must be configured together"
        )),
    }
}

fn validate_send_host(host: &KnownHost, context: &str) -> Result<()> {
    if host.app() != Some(Application::Elasticsearch) || !host.has_role(HostRole::Send) {
        return Err(eyre!("{context} must be an Elasticsearch host with role 'send'"));
    }
    Ok(())
}

fn validate_view_host(host: &KnownHost, name: &str) -> Result<()> {
    if host.app() != Some(Application::Kibana) || !host.has_role(HostRole::View) {
        return Err(eyre!(
            "Output deployment viewer '{name}' must be a Kibana host with role 'view'"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{OutputDeployment, OutputDeploymentSource};
    use crate::data::{Application, ApplicationConfig, Auth, ElasticOutputContext, HostRole, KnownHostBuilder};
    use std::collections::BTreeMap;
    use url::Url;

    fn clear_output_environment(env: &mut crate::TestEnv) {
        for name in [
            "ESDIAG_OUTPUT_URL",
            "ESDIAG_OUTPUT_APIKEY",
            "ESDIAG_OUTPUT_USERNAME",
            "ESDIAG_OUTPUT_PASSWORD",
            "ESDIAG_KIBANA_URL",
        ] {
            env.remove(name);
        }
    }

    fn save_linked_hosts() {
        let output = KnownHostBuilder::new(Url::parse("https://saved-es.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .viewer(Some("saved-kibana".to_string()))
            .build()
            .expect("output");
        let viewer = KnownHostBuilder::new(Url::parse("https://saved-kb.example:5601").expect("url"))
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .expect("viewer");
        crate::data::KnownHost::write_hosts_yml(&BTreeMap::from([
            ("saved-output".to_string(), output),
            ("saved-kibana".to_string(), viewer),
        ]))
        .expect("write hosts");
        ApplicationConfig {
            version: 1,
            output: crate::data::OutputConfig {
                default: Some("saved-output".to_string()),
                ..crate::data::OutputConfig::default()
            },
            ..ApplicationConfig::new()
        }
        .save()
        .expect("save app config");
    }

    #[test]
    fn complete_environment_deployment_precedes_persisted_output() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        env.set("ESDIAG_OUTPUT_URL", "https://runtime-es.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-key");
        env.set("ESDIAG_KIBANA_URL", "https://runtime-kb.example:5601");

        let deployment = OutputDeployment::resolve(None, true).expect("environment deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Environment);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://runtime-es.example:9200/")
        );
        assert_eq!(
            deployment
                .kibana
                .as_ref()
                .and_then(|host| host.concrete_url())
                .map(Url::as_str),
            Some("https://runtime-kb.example:5601/")
        );
    }

    #[test]
    fn partial_environment_never_borrows_persisted_viewer() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        env.set("ESDIAG_OUTPUT_URL", "https://runtime-es.example:9200");

        let err = OutputDeployment::resolve(None, true).expect_err("missing Kibana config must fail");

        assert!(err.to_string().contains("ESDIAG_KIBANA_URL is not defined"));
        assert!(!err.to_string().contains("saved-kibana"));
    }

    #[test]
    fn persisted_output_resolves_its_linked_viewer() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();

        let deployment = OutputDeployment::resolve(None, true).expect("saved deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Persisted);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://saved-es.example:9200/")
        );
        assert_eq!(
            deployment
                .kibana
                .as_ref()
                .and_then(|host| host.concrete_url())
                .map(Url::as_str),
            Some("https://saved-kb.example:5601/")
        );
    }

    #[test]
    fn credentials_without_an_output_url_do_not_override_persisted_output() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-key");

        let deployment = OutputDeployment::resolve(None, true).expect("persisted deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Persisted);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://saved-es.example:9200/")
        );
    }

    #[test]
    fn active_input_context_is_not_an_implicit_output() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        env.set("ESDIAG_ELASTIC_CLI", "1");
        env.set("ELASTIC_ES_URL", "https://customer.example:9200");
        env.set("ELASTIC_ES_API_KEY", "customer-key");

        let error = OutputDeployment::resolve(None, false).expect_err("active input must not become output");

        assert!(error.to_string().contains("No output deployment is configured"));
        assert!(!error.to_string().contains("customer.example"));
        assert!(!error.to_string().contains("customer-key"));
    }

    #[test]
    fn omitted_cli_output_uses_the_persisted_deployment() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();

        let uri = crate::data::Uri::try_from(Option::<String>::None).expect("default output URI");

        assert!(matches!(
            uri,
            crate::data::Uri::KnownHost(host)
                if host.concrete_url().map(Url::as_str) == Some("https://saved-es.example:9200/")
        ));
    }

    #[test]
    fn explicit_target_precedes_runtime_and_persisted_deployments() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        env.set("ESDIAG_OUTPUT_URL", "https://runtime-es.example:9200");
        env.set("ESDIAG_KIBANA_URL", "https://runtime-kb.example:5601");

        let deployment = OutputDeployment::resolve(Some("saved-output"), true).expect("explicit deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Explicit);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://saved-es.example:9200/")
        );
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn configured_context_resolves_one_deployment_with_separate_credentials() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        env.set("MONITORING_ES_KEY", "es-key");
        env.set("MONITORING_KB_KEY", "kb-key");
        let elasticrc_path = env.tmp.path().join(".elasticrc.yml");
        std::fs::write(
            &elasticrc_path,
            "current_context: monitoring\ncontexts:\n  monitoring:\n    elasticsearch:\n      url: https://monitoring-es.example:9200\n      auth:\n        api_key: $(env:MONITORING_ES_KEY)\n    kibana:\n      url: https://monitoring-kb.example:5601\n      auth:\n        api_key: $(env:MONITORING_KB_KEY)\n",
        )
        .expect("write elasticrc");
        ApplicationConfig {
            version: 1,
            output: crate::data::OutputConfig {
                elastic_context: Some(ElasticOutputContext {
                    context: "monitoring".to_string(),
                    config_file: Some(elasticrc_path),
                }),
                ..crate::data::OutputConfig::default()
            },
            ..ApplicationConfig::new()
        }
        .save()
        .expect("save app config");

        let deployment = OutputDeployment::resolve(None, true).expect("context deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::ElasticContext);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://monitoring-es.example:9200/")
        );
        assert_eq!(
            deployment
                .kibana
                .as_ref()
                .and_then(|host| host.concrete_url())
                .map(Url::as_str),
            Some("https://monitoring-kb.example:5601/")
        );
        assert!(matches!(
            deployment.elasticsearch_auth,
            Auth::Apikey(ref key) if key.expose_secret() == "es-key"
        ));
        assert!(matches!(
            deployment.kibana_auth,
            Some(Auth::Apikey(ref key)) if key.expose_secret() == "kb-key"
        ));

        env.set("MONITORING_ES_KEY", "rotated-es-key");
        let rotated = OutputDeployment::resolve(None, false).expect("rotated context deployment");
        assert!(matches!(
            rotated.elasticsearch_auth,
            Auth::Apikey(ref key) if key.expose_secret() == "rotated-es-key"
        ));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn explicit_context_target_precedes_configured_context() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        let elasticrc_path = env.tmp.path().join(".elasticrc.yml");
        std::fs::write(
            &elasticrc_path,
            "current_context: incident\ncontexts:\n  monitoring:\n    elasticsearch:\n      url: https://monitoring.example:9200\n  incident:\n    elasticsearch:\n      url: https://incident.example:9200\n",
        )
        .expect("write elasticrc");
        env.set_path("ELASTIC_CLI_CONFIG_FILE", elasticrc_path);
        ApplicationConfig {
            version: 1,
            output: crate::data::OutputConfig {
                elastic_context: Some(ElasticOutputContext {
                    context: "monitoring".to_string(),
                    config_file: None,
                }),
                ..crate::data::OutputConfig::default()
            },
            ..ApplicationConfig::new()
        }
        .save()
        .expect("save app config");

        let deployment = OutputDeployment::resolve(Some(".incident.es"), false).expect("explicit context");

        assert_eq!(deployment.source, OutputDeploymentSource::Explicit);
        assert_eq!(
            deployment.elasticsearch.concrete_url().map(Url::as_str),
            Some("https://incident.example:9200/")
        );
    }
}
