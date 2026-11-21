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
        Self::from_persisted(require_kibana)
    }

    /// Resolve the named or persisted saved output without consulting `ESDIAG_OUTPUT_*`.
    ///
    /// With `include_viewer`, the linked Kibana viewer is resolved too when the output has one;
    /// without it, a missing viewer host or viewer credential cannot block Elasticsearch-only work.
    pub fn resolve_saved(explicit_target: Option<&str>, include_viewer: bool) -> Result<Self> {
        let mut deployment = match explicit_target {
            Some(target) => Self::from_explicit(target, false)?,
            None => Self::from_persisted(false)?,
        };
        if !include_viewer {
            return Ok(deployment);
        }
        if let Some((viewer, auth)) = linked_viewer(&deployment.elasticsearch)? {
            deployment.elasticsearch = deployment.elasticsearch.with_attached_viewer(viewer.clone());
            deployment.kibana = Some(viewer);
            deployment.kibana_auth = Some(auth);
        }
        Ok(deployment)
    }

    fn from_explicit(target: &str, require_kibana: bool) -> Result<Self> {
        if let Some(target) = ElasticContextTarget::parse(target)? {
            let (elasticsearch, elasticsearch_auth, kibana, kibana_auth) =
                target.resolve_output_hosts(require_kibana)?;
            return Ok(Self::assemble(
                OutputDeploymentSource::Explicit,
                elasticsearch,
                elasticsearch_auth,
                kibana,
                kibana_auth,
            ));
        }
        let output = KnownHost::get_known(&target.to_string())
            .ok_or_else(|| eyre!("Explicit output deployment target '{target}' must be a saved Elasticsearch host"))?;
        Self::from_saved(OutputDeploymentSource::Explicit, output, require_kibana)
    }

    /// The persisted output's linked Kibana viewer, resolved without the output's own Elasticsearch credential.
    pub fn persisted_viewer() -> Result<Option<KnownHost>> {
        let output = persisted_output()?;
        validate_send_host(&output, "output deployment")?;
        Ok(linked_viewer(&output)?.map(|(viewer, _)| viewer))
    }

    fn from_persisted(require_kibana: bool) -> Result<Self> {
        Self::from_saved(OutputDeploymentSource::Persisted, persisted_output()?, require_kibana)
    }

    pub fn from_elastic_context(context: &ElasticOutputContext, require_kibana: bool) -> Result<Self> {
        let (elasticsearch, elasticsearch_auth, kibana, kibana_auth) = context.resolve_output_hosts(require_kibana)?;
        Ok(Self::assemble(
            OutputDeploymentSource::ElasticContext,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        ))
    }

    fn assemble(
        source: OutputDeploymentSource,
        elasticsearch: KnownHost,
        elasticsearch_auth: Auth,
        kibana: Option<KnownHost>,
        kibana_auth: Option<Auth>,
    ) -> Self {
        let elasticsearch = match &kibana {
            Some(kibana) => elasticsearch.with_attached_viewer(kibana.clone()),
            None => elasticsearch,
        };
        Self {
            source,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        }
    }

    fn from_saved(source: OutputDeploymentSource, elasticsearch: KnownHost, require_kibana: bool) -> Result<Self> {
        validate_send_host(&elasticsearch, "output deployment")?;
        let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;

        let (kibana, kibana_auth) = if require_kibana {
            if elasticsearch.viewer().is_none() {
                return Err(eyre!("Output deployment requires a linked Kibana viewer"));
            }
            let (viewer, auth) = linked_viewer(&elasticsearch)?.expect("viewer link checked");
            (Some(viewer), Some(auth))
        } else {
            (None, None)
        };

        Ok(Self::assemble(
            source,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        ))
    }

    /// Resolve the output declared by `ESDIAG_OUTPUT_*`. `ESDIAG_KIBANA_URL`
    /// supplies its viewer when set, and is mandatory when `require_kibana`.
    pub(crate) fn from_environment(require_kibana: bool) -> Result<Self> {
        let output_url = required_environment("ESDIAG_OUTPUT_URL")?;
        let auth = environment_auth()?;
        let elasticsearch = KnownHostBuilder::new(Url::parse(&output_url)?)
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .auth(auth.clone())
            .build()?;
        let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;

        let kibana_url = if require_kibana {
            Some(required_environment("ESDIAG_KIBANA_URL")?)
        } else {
            env::var("ESDIAG_KIBANA_URL")
                .ok()
                .filter(|value| !value.trim().is_empty())
        };
        let kibana = kibana_url
            .map(|kibana_url| -> Result<(KnownHost, Auth)> {
                let kibana = KnownHostBuilder::new(
                    Url::parse(&kibana_url).map_err(|err| eyre!("ESDIAG_KIBANA_URL is not a valid URL: {err}"))?,
                )
                .application(Application::Kibana)
                .roles(vec![HostRole::View])
                .auth(auth)
                .build()?;
                let auth = kibana.get_auth_for_direction(CredentialDirection::Output)?;
                Ok((kibana, auth))
            })
            .transpose();
        let (kibana, kibana_auth) = match kibana {
            Ok(Some((kibana, auth))) => (Some(kibana), Some(auth)),
            Ok(None) => (None, None),
            Err(err) if !require_kibana => {
                tracing::warn!("Ignoring ESDIAG_KIBANA_URL; exports will omit Kibana links: {err}");
                (None, None)
            }
            Err(err) => return Err(err),
        };

        Ok(Self::assemble(
            OutputDeploymentSource::Environment,
            elasticsearch,
            elasticsearch_auth,
            kibana,
            kibana_auth,
        ))
    }
}

pub fn runtime_output_is_declared() -> bool {
    env::var_os("ESDIAG_OUTPUT_URL").is_some()
}

fn required_environment(name: &str) -> Result<String> {
    let value = env::var(name).map_err(|_| eyre!("{name} is not defined"))?;
    if value.trim().is_empty() {
        return Err(eyre!("{name} is empty"));
    }
    Ok(value)
}

pub(crate) fn environment_auth() -> Result<Auth> {
    output_auth_from(|name| env::var(name).ok())
}

/// Read `ESDIAG_OUTPUT_APIKEY`, `_USERNAME`, and `_PASSWORD` through `lookup` and validate the combination.
pub fn output_auth_from(lookup: impl Fn(&str) -> Option<String>) -> Result<Auth> {
    let read = |name| lookup(name).filter(|value: &String| !value.trim().is_empty());
    let apikey = read("ESDIAG_OUTPUT_APIKEY");
    let username = read("ESDIAG_OUTPUT_USERNAME");
    let password = read("ESDIAG_OUTPUT_PASSWORD");

    match (apikey, username, password) {
        (Some(apikey), None, None) => Ok(Auth::apikey(apikey)),
        (None, Some(username), Some(password)) => Ok(Auth::basic(username, password)),
        (None, None, None) => Ok(Auth::None),
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

fn persisted_output() -> Result<KnownHost> {
    let config = ApplicationConfig::load()?;
    let output_name = config.output.default.ok_or_else(|| {
        eyre!("No output deployment is configured. Run `esdiag init` or provide an explicit output target.")
    })?;
    KnownHost::get_known(&output_name).ok_or_else(|| eyre!("Configured output host '{output_name}' was not found"))
}

fn linked_viewer(elasticsearch: &KnownHost) -> Result<Option<(KnownHost, Auth)>> {
    let Some(viewer_name) = elasticsearch.viewer() else {
        return Ok(None);
    };
    let viewer = KnownHost::get_known(&viewer_name.to_string())
        .ok_or_else(|| eyre!("Output deployment viewer '{viewer_name}' was not found"))?;
    validate_view_host(&viewer, viewer_name)?;
    let auth = viewer.get_auth_for_direction(CredentialDirection::Output)?;
    Ok(Some((viewer, auth)))
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

    fn concrete(host: Option<&crate::data::KnownHost>) -> Option<String> {
        host.and_then(|host| host.concrete_url()).map(Url::to_string)
    }

    #[test]
    fn resolve_saved_ignores_a_complete_environment_deployment() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        env.set("ESDIAG_OUTPUT_URL", "https://runtime-es.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-key");
        env.set("ESDIAG_KIBANA_URL", "https://runtime-kb.example:5601");

        let deployment = OutputDeployment::resolve_saved(None, true).expect("persisted deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Persisted);
        assert_eq!(
            concrete(Some(&deployment.elasticsearch)).as_deref(),
            Some("https://saved-es.example:9200/")
        );
        assert_eq!(
            concrete(deployment.kibana.as_ref()).as_deref(),
            Some("https://saved-kb.example:5601/")
        );
    }

    #[test]
    fn resolve_saved_uses_a_named_output_and_tolerates_no_viewer() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();
        let lab = KnownHostBuilder::new(Url::parse("https://lab-es.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .build()
            .expect("lab output");
        let mut hosts = crate::data::KnownHost::parse_hosts_yml().expect("hosts");
        hosts.insert("lab-output".to_string(), lab);
        crate::data::KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let deployment = OutputDeployment::resolve_saved(Some("lab-output"), true).expect("named deployment");

        assert_eq!(deployment.source, OutputDeploymentSource::Explicit);
        assert_eq!(
            concrete(Some(&deployment.elasticsearch)).as_deref(),
            Some("https://lab-es.example:9200/")
        );
        assert!(deployment.kibana.is_none());
    }

    #[test]
    fn resolve_saved_rejects_a_viewer_name() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        save_linked_hosts();

        let error = OutputDeployment::resolve_saved(Some("saved-kibana"), true).expect_err("viewer is not an output");

        assert!(error.to_string().contains("role 'send'"), "{error}");
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
        env.set_path("ELASTIC_CLI_CONFIG_FILE", elasticrc_path);
        ApplicationConfig {
            version: 1,
            output: crate::data::OutputConfig {
                elastic_context: Some(ElasticOutputContext {
                    context: "monitoring".to_string(),
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

    #[test]
    fn invalid_optional_kibana_url_does_not_block_elasticsearch_output() {
        let mut env = crate::TestEnv::new();
        clear_output_environment(&mut env);
        env.set("ESDIAG_OUTPUT_URL", "https://runtime-es.example:9200");
        env.set("ESDIAG_KIBANA_URL", "not a url");

        let deployment = OutputDeployment::from_environment(false).expect("Elasticsearch-only output");
        assert!(deployment.kibana.is_none());
        assert!(deployment.elasticsearch.attached_viewer().is_none());

        let error = OutputDeployment::from_environment(true).expect_err("Kibana is required");
        assert!(
            error.to_string().contains("ESDIAG_KIBANA_URL is not a valid URL"),
            "{error}"
        );
    }
}
