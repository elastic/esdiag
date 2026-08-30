// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{Application, Auth, KnownHost};
#[cfg(feature = "elasticrc")]
use super::{CredentialDirection, HostRole, KnownHostBuilder};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
#[cfg(feature = "elasticrc")]
use std::path::Path;
use std::{
    env,
    fmt::{Display, Formatter},
    path::PathBuf,
    str::FromStr,
};
#[cfg(feature = "elasticrc")]
use url::Url;

#[cfg(feature = "elasticrc")]
use std::{
    collections::HashMap,
    ffi::OsString,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Mutex, MutexGuard, OnceLock},
};

/// A stable symbolic reference to one application in an Elastic CLI context.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ElasticContextTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub application: Application,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_deployment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<PathBuf>,
}

/// A stable symbolic reference to a named Elastic CLI output deployment.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ElasticOutputContext {
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<PathBuf>,
}

impl ElasticOutputContext {
    pub fn new(context: impl Into<String>) -> Result<Self> {
        let context = context.into();
        if context.trim().is_empty() {
            return Err(eyre!("Elastic CLI output context name must not be empty"));
        }
        Ok(Self {
            context,
            config_file: env::var_os("ELASTIC_CLI_CONFIG_FILE").map(PathBuf::from),
        })
    }
}

impl Display for ElasticOutputContext {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.context)
    }
}

impl ElasticContextTarget {
    /// Parse `.service`, `.context.service`, or a deployment-qualified Cloud reference.
    ///
    /// Unknown leading-dot values return `Ok(None)` so existing hidden-file and
    /// saved-host resolution can continue. Recognized but incomplete Cloud
    /// references return an error.
    pub fn parse(value: &str) -> Result<Option<Self>> {
        let Some(reference) = value.strip_prefix('.') else {
            return Ok(None);
        };
        if reference.is_empty() || reference.contains('\\') {
            return Ok(None);
        }

        let segments = reference.split('/').collect::<Vec<_>>();
        if segments.len() > 1 {
            return Self::parse_cloud_segments(&segments);
        }

        let (context, service) = split_context_service(segments[0])?;
        let application = match service {
            "es" | "elasticsearch" => Application::Elasticsearch,
            "kb" | "kibana" => Application::Kibana,
            "cloud" => {
                return Err(eyre!(
                    "Elastic Cloud target '{value}' requires a deployment identifier and application, for example '{value}/<deployment-id>/es'"
                ));
            }
            _ => return Ok(None),
        };

        Ok(Some(Self {
            context,
            application,
            cloud_deployment: None,
            config_file: env::var_os("ELASTIC_CLI_CONFIG_FILE").map(PathBuf::from),
        }))
    }

    fn parse_cloud_segments(segments: &[&str]) -> Result<Option<Self>> {
        let Some(base) = segments.first() else {
            return Ok(None);
        };
        let (context, service) = split_context_service(base)?;
        if service != "cloud" {
            return Ok(None);
        }
        if segments.len() != 3 || segments[1].is_empty() || segments[2].is_empty() {
            return Err(eyre!(
                "Elastic Cloud targets require '.cloud/<deployment-id>/<application>' or '.context.cloud/<deployment-id>/<application>'"
            ));
        }
        let application = Application::from_str(segments[2])
            .map_err(|_| eyre!("Unsupported Elastic Cloud proxy application '{}'", segments[2]))?;
        if application != Application::Elasticsearch {
            return Err(eyre!(
                "Elastic Cloud proxy application '{}' is not supported; use 'es' or 'elasticsearch'",
                segments[2]
            ));
        }

        Ok(Some(Self {
            context,
            application,
            cloud_deployment: Some(segments[1].to_string()),
            config_file: env::var_os("ELASTIC_CLI_CONFIG_FILE").map(PathBuf::from),
        }))
    }

    pub fn is_cloud_admin(&self) -> bool {
        self.cloud_deployment.is_some()
    }

    pub fn resolve_collect_host(&self) -> Result<KnownHost> {
        #[cfg(feature = "elasticrc")]
        {
            if let Some(deployment_id) = &self.cloud_deployment {
                let service = resolve_service(self, elasticrc::ServiceKind::Cloud)?;
                let mut url = service.url;
                url.set_path(&format!("/deployments/{deployment_id}"));
                return known_host_from_service(
                    elasticrc::ResolvedService {
                        kind: elasticrc::ServiceKind::Cloud,
                        url,
                        auth: service.auth,
                    },
                    self.application,
                    vec![HostRole::Collect],
                );
            }
            let service = resolve_service(self, service_kind(self.application)?)?;
            known_host_from_service(service, self.application, vec![HostRole::Collect])
        }
        #[cfg(not(feature = "elasticrc"))]
        {
            Err(eyre!(
                "Elastic CLI context target '{}' requires the 'elasticrc' feature",
                self
            ))
        }
    }

    pub(crate) fn resolve_output_hosts(
        &self,
        require_kibana: bool,
    ) -> Result<(KnownHost, Auth, Option<KnownHost>, Option<Auth>)> {
        if self.cloud_deployment.is_some() {
            return Err(eyre!(
                "Elastic Cloud admin references cannot be used as output deployments"
            ));
        }
        if self.application != Application::Elasticsearch {
            return Err(eyre!(
                "Elastic CLI output targets must select the Elasticsearch application"
            ));
        }

        #[cfg(feature = "elasticrc")]
        {
            resolve_output_hosts(self.context.as_deref(), self.config_file.as_deref(), require_kibana)
        }
        #[cfg(not(feature = "elasticrc"))]
        {
            let _ = require_kibana;
            Err(eyre!(
                "Elastic CLI context target '{}' requires the 'elasticrc' feature",
                self
            ))
        }
    }
}

impl Display for ElasticContextTarget {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(".")?;
        if let Some(context) = &self.context {
            write!(formatter, "{context}.")?;
        }
        if let Some(deployment) = &self.cloud_deployment {
            write!(formatter, "cloud/{deployment}/{}", self.application.key())
        } else {
            formatter.write_str(self.application.key())
        }
    }
}

fn split_context_service(value: &str) -> Result<(Option<String>, &str)> {
    match value.rsplit_once('.') {
        Some((context, service)) if !context.is_empty() && !service.is_empty() => {
            Ok((Some(context.to_string()), service))
        }
        Some(_) => Err(eyre!("Elastic CLI context reference has an empty context or service")),
        None => Ok((None, value)),
    }
}

#[cfg(feature = "elasticrc")]
fn service_kind(application: Application) -> Result<elasticrc::ServiceKind> {
    match application {
        Application::Elasticsearch => Ok(elasticrc::ServiceKind::Elasticsearch),
        Application::Kibana => Ok(elasticrc::ServiceKind::Kibana),
        Application::Logstash | Application::Agent => Err(eyre!(
            "Application '{}' is not supported by Elastic CLI context targets",
            application
        )),
    }
}

#[cfg(feature = "elasticrc")]
fn resolve_service(target: &ElasticContextTarget, kind: elasticrc::ServiceKind) -> Result<elasticrc::ResolvedService> {
    if target.context.is_none()
        && crate::env::is_elastic_cli_invocation()
        && let Some(service) = service_from_active_environment(kind)?
    {
        return Ok(service);
    }

    let config = load_config_cached(target.config_file.as_deref())?
        .ok_or_else(|| eyre!("Elastic CLI config was not found for context target '{target}'"))?;
    match target.context.as_deref() {
        Some(context) => config.resolve_service(context, kind).map_err(Into::into),
        None => config.resolve_current_service(kind).map_err(Into::into),
    }
}

#[cfg(feature = "elasticrc")]
fn resolve_output_hosts(
    context: Option<&str>,
    config_file: Option<&Path>,
    require_kibana: bool,
) -> Result<(KnownHost, Auth, Option<KnownHost>, Option<Auth>)> {
    if context.is_none()
        && crate::env::is_elastic_cli_invocation()
        && let Some(es) = service_from_active_environment(elasticrc::ServiceKind::Elasticsearch)?
    {
        let elasticsearch = known_host_from_service(es, Application::Elasticsearch, vec![HostRole::Send])?;
        let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;
        let kibana = service_from_active_environment(elasticrc::ServiceKind::Kibana)?
            .map(|service| known_host_from_service(service, Application::Kibana, vec![HostRole::View]))
            .transpose()?;
        if require_kibana && kibana.is_none() {
            return Err(eyre!("Active Elastic CLI output context has no Kibana service"));
        }
        let kibana_auth = kibana
            .as_ref()
            .map(|host| host.get_auth_for_direction(CredentialDirection::Output))
            .transpose()?;
        return Ok((elasticsearch, elasticsearch_auth, kibana, kibana_auth));
    }

    let config = load_config_cached(config_file)?
        .ok_or_else(|| eyre!("Elastic CLI config was not found while resolving the output deployment"))?;
    let context_name = context.unwrap_or(&config.current_context);
    let es = config.resolve_service(context_name, elasticrc::ServiceKind::Elasticsearch)?;
    let elasticsearch = known_host_from_service(es, Application::Elasticsearch, vec![HostRole::Send])?;
    let elasticsearch_auth = elasticsearch.get_auth_for_direction(CredentialDirection::Output)?;
    let kibana = match config.resolve_service(context_name, elasticrc::ServiceKind::Kibana) {
        Ok(service) => Some(known_host_from_service(
            service,
            Application::Kibana,
            vec![HostRole::View],
        )?),
        Err(elasticrc::Error::MissingService { .. }) if !require_kibana => None,
        Err(error) => return Err(error.into()),
    };
    let kibana_auth = kibana
        .as_ref()
        .map(|host| host.get_auth_for_direction(CredentialDirection::Output))
        .transpose()?;
    Ok((elasticsearch, elasticsearch_auth, kibana, kibana_auth))
}

#[cfg(feature = "elasticrc")]
fn known_host_from_service(
    service: elasticrc::ResolvedService,
    application: Application,
    roles: Vec<HostRole>,
) -> Result<KnownHost> {
    let cloud_admin = service.kind == elasticrc::ServiceKind::Cloud;
    let gov_cloud = service.url.domain() == Some("admin.us-gov-east-1.aws.elastic-cloud.com");
    let mut builder = KnownHostBuilder::new(service.url).application(application).roles(roles);
    match service.auth {
        elasticrc::ResolvedAuth::ApiKey(api_key) => {
            builder = builder.apikey(Some(api_key.expose_secret().clone()));
        }
        elasticrc::ResolvedAuth::Basic { username, password } => {
            builder = builder
                .username(Some(username))
                .password(Some(password.expose_secret().clone()));
        }
        elasticrc::ResolvedAuth::None => {}
    }
    let mut host = builder.build()?;
    if cloud_admin {
        host.cloud_id = Some(if gov_cloud {
            super::ElasticCloud::ElasticGovCloudAdmin
        } else {
            super::ElasticCloud::ElasticCloudAdmin
        });
    }
    Ok(host)
}

#[cfg(feature = "elasticrc")]
fn service_from_active_environment(kind: elasticrc::ServiceKind) -> Result<Option<elasticrc::ResolvedService>> {
    let (url_name, api_key_name, username_name, password_name) = match kind {
        elasticrc::ServiceKind::Elasticsearch => (
            "ELASTIC_ES_URL",
            "ELASTIC_ES_API_KEY",
            "ELASTIC_ES_USERNAME",
            "ELASTIC_ES_PASSWORD",
        ),
        elasticrc::ServiceKind::Kibana => (
            "ELASTIC_KIBANA_URL",
            "ELASTIC_KIBANA_API_KEY",
            "ELASTIC_KIBANA_USERNAME",
            "ELASTIC_KIBANA_PASSWORD",
        ),
        elasticrc::ServiceKind::Cloud => (
            "ELASTIC_CLOUD_URL",
            "ELASTIC_CLOUD_API_KEY",
            "ELASTIC_CLOUD_USERNAME",
            "ELASTIC_CLOUD_PASSWORD",
        ),
    };
    let Some(url) = env::var(url_name).ok().filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let api_key = env::var(api_key_name).ok().filter(|value| !value.trim().is_empty());
    let username = env::var(username_name).ok().filter(|value| !value.trim().is_empty());
    let password = env::var(password_name).ok().filter(|value| !value.trim().is_empty());
    let auth = match (api_key, username, password) {
        (Some(api_key), None, None) => elasticrc::ResolvedAuth::api_key(api_key),
        (None, Some(username), Some(password)) => elasticrc::ResolvedAuth::basic(username, password),
        (None, None, None) => elasticrc::ResolvedAuth::None,
        (Some(_), _, _) => {
            return Err(eyre!(
                "{api_key_name} cannot be combined with {username_name} or {password_name}"
            ));
        }
        (None, Some(_), None) | (None, None, Some(_)) => {
            return Err(eyre!("{username_name} and {password_name} must be configured together"));
        }
    };
    Ok(Some(elasticrc::ResolvedService {
        kind,
        url: Url::parse(&url)?,
        auth,
    }))
}

#[cfg(feature = "elasticrc")]
#[derive(Clone, Eq, Hash, PartialEq)]
struct ConfigCacheKey {
    explicit_path: Option<OsString>,
    home: Option<OsString>,
    user_profile: Option<OsString>,
}

#[cfg(feature = "elasticrc")]
#[derive(Clone, Eq, PartialEq)]
struct ConfigFingerprint {
    path: PathBuf,
    content_hash: u64,
}

#[cfg(feature = "elasticrc")]
#[derive(Clone)]
struct CachedConfig {
    fingerprint: ConfigFingerprint,
    config: Box<elasticrc::ConfigFile>,
}

#[cfg(feature = "elasticrc")]
fn config_cache() -> &'static Mutex<HashMap<ConfigCacheKey, CachedConfig>> {
    static CACHE: OnceLock<Mutex<HashMap<ConfigCacheKey, CachedConfig>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(feature = "elasticrc")]
fn config_cache_lock() -> Result<MutexGuard<'static, HashMap<ConfigCacheKey, CachedConfig>>> {
    config_cache()
        .lock()
        .map_err(|error| eyre!("elasticrc cache lock poisoned: {error}"))
}

#[cfg(feature = "elasticrc")]
fn load_config_cached(explicit_path: Option<&Path>) -> Result<Option<elasticrc::ConfigFile>> {
    let explicit_path = explicit_path
        .map(Path::as_os_str)
        .map(ToOwned::to_owned)
        .or_else(|| env::var_os("ELASTIC_CLI_CONFIG_FILE"));
    let key = ConfigCacheKey {
        explicit_path: explicit_path.clone(),
        home: env::var_os("HOME"),
        user_profile: env::var_os("USERPROFILE"),
    };
    let selected_path = explicit_path.as_deref().map(PathBuf::from).or_else(|| {
        key.home
            .as_deref()
            .or(key.user_profile.as_deref())
            .and_then(|home| elasticrc::discover_config_path(Path::new(home)))
    });
    let fingerprint = selected_path.as_deref().and_then(config_fingerprint);
    if let Some(fingerprint) = &fingerprint {
        let cache = config_cache_lock()?;
        if let Some(cached) = cache.get(&key)
            && &cached.fingerprint == fingerprint
        {
            return Ok(Some((*cached.config).clone()));
        }
    }

    let loaded = match elasticrc::ConfigFile::load_with_options(explicit_path.as_deref().map(Path::new), None) {
        Ok(config) => Some(config),
        Err(elasticrc::Error::ConfigNotFound { .. }) | Err(elasticrc::Error::HomeDirectoryUnavailable) => None,
        Err(error) => return Err(error.into()),
    };
    if let (Some(config), Some(fingerprint)) = (&loaded, fingerprint) {
        config_cache_lock()?.insert(
            key,
            CachedConfig {
                fingerprint,
                config: Box::new(config.clone()),
            },
        );
    } else {
        config_cache_lock()?.remove(&key);
    }
    Ok(loaded)
}

#[cfg(feature = "elasticrc")]
fn config_fingerprint(path: &Path) -> Option<ConfigFingerprint> {
    let contents = std::fs::read(path).ok()?;
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Some(ConfigFingerprint {
        path: path.to_path_buf(),
        content_hash: hasher.finish(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Application, ElasticContextTarget};
    use crate::data::Auth;

    #[test]
    fn parses_direct_active_and_named_targets() {
        let active = ElasticContextTarget::parse(".es").expect("parse").expect("target");
        assert_eq!(active.context, None);
        assert_eq!(active.application, Application::Elasticsearch);

        let named = ElasticContextTarget::parse(".prod.us-west.kb")
            .expect("parse")
            .expect("target");
        assert_eq!(named.context.as_deref(), Some("prod.us-west"));
        assert_eq!(named.application, Application::Kibana);
    }

    #[test]
    fn cloud_target_requires_deployment_and_application() {
        for value in [".cloud", ".prod.cloud", ".prod.cloud/deployment-123"] {
            assert!(ElasticContextTarget::parse(value).is_err(), "{value} must fail");
        }
    }

    #[test]
    fn parses_explicit_cloud_elasticsearch_target() {
        let target = ElasticContextTarget::parse(".prod.cloud/deployment-123/es")
            .expect("parse")
            .expect("target");

        assert_eq!(target.context.as_deref(), Some("prod"));
        assert_eq!(target.application, Application::Elasticsearch);
        assert_eq!(target.cloud_deployment.as_deref(), Some("deployment-123"));
        assert_eq!(target.to_string(), ".prod.cloud/deployment-123/elasticsearch");
    }

    #[test]
    fn cloud_target_rejects_unsupported_application() {
        let error = ElasticContextTarget::parse(".prod.cloud/deployment-123/kb").expect_err("Kibana must fail");
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn cloud_admin_target_is_not_an_output_deployment() {
        let target = ElasticContextTarget::parse(".prod.cloud/deployment-123/es")
            .expect("parse")
            .expect("target");

        let error = target.resolve_output_hosts(false).expect_err("Cloud output must fail");

        assert!(error.to_string().contains("cannot be used as output deployments"));
    }

    #[test]
    fn unknown_leading_dot_value_falls_through() {
        assert_eq!(ElasticContextTarget::parse(".prod.ls").expect("parse"), None);
        assert_eq!(ElasticContextTarget::parse("./.es").expect("parse"), None);
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn active_elasticsearch_target_uses_input_credentials_only() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_ELASTIC_CLI", "1");
        env.set("ELASTIC_ES_URL", "https://active.example:9200");
        env.set("ELASTIC_ES_API_KEY", "input-key");
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "output-key");
        let target = ElasticContextTarget::parse(".es").expect("parse").expect("target");

        let host = target.resolve_collect_host().expect("resolve");

        assert_eq!(
            host.concrete_url().map(url::Url::as_str),
            Some("https://active.example:9200/")
        );
        assert!(matches!(
            host.get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "input-key"
        ));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_target_reports_missing_context() {
        let env = crate::TestEnv::new();
        let config_file = env.tmp.path().join("missing.yml");
        std::fs::write(
            &config_file,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://prod.example:9200\n",
        )
        .expect("write config");
        let mut target = ElasticContextTarget::parse(".diag.es").expect("parse").expect("target");
        target.config_file = Some(config_file);

        let error = target.resolve_collect_host().expect_err("missing context must fail");

        assert!(error.to_string().contains("Elastic CLI context 'diag' was not found"));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn cached_config_reloads_when_file_contents_change() {
        let mut env = crate::TestEnv::new();
        let config_file = env.tmp.path().join("rotating.yml");
        let write_config = |url: &str, key: &str| {
            std::fs::write(
                &config_file,
                format!(
                    "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: {url}\n      auth:\n        api_key: {key}\n"
                ),
            )
            .expect("write config");
        };
        write_config("https://one.example:9200", "first-key");
        env.set_path("ELASTIC_CLI_CONFIG_FILE", config_file.clone());
        let target = ElasticContextTarget::parse(".prod.es").expect("parse").expect("target");
        let first = target.resolve_collect_host().expect("first resolution");
        assert_eq!(
            first.concrete_url().map(url::Url::as_str),
            Some("https://one.example:9200/")
        );

        write_config("https://two.example:9200", "other-key");
        let second = target.resolve_collect_host().expect("reloaded resolution");

        assert_eq!(
            second.concrete_url().map(url::Url::as_str),
            Some("https://two.example:9200/")
        );
        assert!(matches!(
            second.get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "other-key"
        ));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_cloud_target_materializes_admin_proxy_route() {
        let env = crate::TestEnv::new();
        let config_file = env.tmp.path().join("cloud.yml");
        std::fs::write(
            &config_file,
            "current_context: prod\ncontexts:\n  prod:\n    cloud:\n      url: https://api.elastic-cloud.com\n      auth:\n        api_key: cloud-key\n",
        )
        .expect("write config");
        let mut target = ElasticContextTarget::parse(".prod.cloud/deployment-123/es")
            .expect("parse")
            .expect("target");
        target.config_file = Some(config_file);

        let host = target.resolve_collect_host().expect("resolve cloud target");
        let resolved = host.resolve().expect("resolve host");

        assert_eq!(resolved.application(), Application::Elasticsearch);
        assert_eq!(resolved.route(), crate::data::HostRoute::ElasticCloudAdmin);
        assert_eq!(
            resolved.as_ref().concrete_url().map(url::Url::as_str),
            Some("https://api.elastic-cloud.com/api/v1/deployments/deployment-123/elasticsearch/_main/proxy/")
        );
        assert!(resolved.as_ref().has_role(crate::data::HostRole::Collect));
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn active_cloud_target_materializes_admin_proxy_route() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_ELASTIC_CLI", "1");
        env.set("ELASTIC_CLOUD_URL", "https://api.elastic-cloud.com");
        env.set("ELASTIC_CLOUD_API_KEY", "active-cloud-key");
        let target = ElasticContextTarget::parse(".cloud/deployment-456/es")
            .expect("parse")
            .expect("target");

        let host = target.resolve_collect_host().expect("resolve cloud target");
        let resolved = host.resolve().expect("resolve host");

        assert_eq!(resolved.application(), Application::Elasticsearch);
        assert_eq!(resolved.route(), crate::data::HostRoute::ElasticCloudAdmin);
        assert_eq!(
            resolved.as_ref().concrete_url().map(url::Url::as_str),
            Some("https://api.elastic-cloud.com/api/v1/deployments/deployment-456/elasticsearch/_main/proxy/")
        );
        assert!(matches!(
            resolved.as_ref().get_auth().expect("auth"),
            Auth::Apikey(key) if key.expose_secret() == "active-cloud-key"
        ));
    }
}
