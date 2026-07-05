// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{data::Product, env};

use super::{ElasticCloud, KnownHost, KnownHostBuilder};
use eyre::{OptionExt, Report, Result, eyre};
use serde::{Deserialize, Deserializer};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use url::Url;
/// The different types of supported URIs
#[derive(Clone, Default)]
pub enum Uri {
    /// Known host saved in the ~/.esdiag/hosts.yml by default
    KnownHost(KnownHost),
    /// An Elastic Cloud URL for the Elasticsearch API proxy
    ElasticCloud(KnownHost),
    /// An Elastic Cloud Admin URL for the Elasticsearch API proxy
    ElasticCloudAdmin(KnownHost),
    /// An Elastic Cloud GovCloud Admin URL for the Elasticsearch API proxy
    ElasticGovCloudAdmin(KnownHost),
    /// An Elastic Uploader service URL, embed the auth token as `token:<value>@` instead of `username:password` in the URL
    ServiceLink(Url),
    /// An Elastic Uploader service URL, without authentication
    ServiceLinkNoAuth(Url),
    /// A standard URL
    Url(Url),
    /// Directory on the local file system
    Directory(PathBuf),
    /// File on the local filesystem
    File(PathBuf),
    /// An input/output stream (stdin/stdout)
    #[default]
    Stream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ElasticCliService {
    Elasticsearch,
    Kibana,
    Cloud,
}

fn parse_active_context_service(reference: &str) -> Option<ElasticCliService> {
    let service = reference.strip_prefix('.')?;
    if service.is_empty() || service.contains('.') {
        return None;
    }

    match service {
        "elasticsearch" | "es" => Some(ElasticCliService::Elasticsearch),
        "kibana" | "kb" => Some(ElasticCliService::Kibana),
        "cloud" => Some(ElasticCliService::Cloud),
        _ => None,
    }
}

fn try_from_active_context_service(service: ElasticCliService) -> Result<Uri> {
    match service {
        ElasticCliService::Elasticsearch => Uri::try_from_active_elasticsearch_env(),
        ElasticCliService::Kibana => Uri::try_from_active_kibana_env(),
        ElasticCliService::Cloud => Uri::try_from_active_cloud_env(),
    }
}

#[cfg(feature = "elasticrc")]
fn uri_from_elasticrc_service(service: elasticrc::ResolvedService) -> Result<Uri> {
    let product = match service.kind {
        elasticrc::ServiceKind::Elasticsearch | elasticrc::ServiceKind::Cloud => Product::Elasticsearch,
        elasticrc::ServiceKind::Kibana => Product::Kibana,
    };
    let mut builder = KnownHostBuilder::new(service.url).product(product);
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
    Ok(builder.build()?.try_into()?)
}

#[cfg(feature = "elasticrc")]
fn try_from_elasticrc_reference(reference: &str) -> Result<Option<Uri>> {
    let Some(reference) = elasticrc::ContextServiceReference::parse(reference) else {
        return Ok(None);
    };
    let Some(context) = reference.context else {
        return Ok(None);
    };

    let config = elasticrc::ConfigFile::load_with_options(None, None)?;
    let service = config.resolve_service(&context, reference.service)?;
    Ok(Some(uri_from_elasticrc_service(service)?))
}

#[cfg(feature = "elasticrc")]
fn try_from_current_elasticrc_reference(reference: &str) -> Result<Option<Uri>> {
    let Some(reference) = elasticrc::ContextServiceReference::parse(reference) else {
        return Ok(None);
    };
    if reference.context.is_some() {
        return Ok(None);
    }

    let config = match elasticrc::ConfigFile::load_with_options(None, None) {
        Ok(config) => config,
        Err(elasticrc::Error::ConfigNotFound { .. }) | Err(elasticrc::Error::HomeDirectoryUnavailable) => {
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };
    let service = config.resolve_current_service(reference.service)?;
    Ok(Some(uri_from_elasticrc_service(service)?))
}

/// Try reading the authentication environment variables.
/// Returns a tuple of optional strings for (apikey, username, password)
fn try_get_auth_env(
    fallback_apikey: &str,
    fallback_username: &str,
    fallback_password: &str,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let esdiag_apikey = std::env::var("ESDIAG_OUTPUT_APIKEY").ok();
    if esdiag_apikey.is_some() {
        return Ok((esdiag_apikey, None, None));
    }

    let esdiag_username = std::env::var("ESDIAG_OUTPUT_USERNAME").ok();
    let esdiag_password = std::env::var("ESDIAG_OUTPUT_PASSWORD").ok();
    if esdiag_username.is_some() || esdiag_password.is_some() {
        return Ok((None, esdiag_username, esdiag_password));
    }

    let apikey = std::env::var(fallback_apikey).ok();
    if apikey.is_some() {
        return Ok((apikey, None, None));
    }

    let username = std::env::var(fallback_username).ok();
    let password = std::env::var(fallback_password).ok();
    Ok((apikey, username, password))
}

fn try_get_elastic_cli_auth_env(
    apikey_name: &str,
    username_name: &str,
    password_name: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let apikey = std::env::var(apikey_name).ok();
    if apikey.is_some() {
        return (apikey, None, None);
    }

    (
        None,
        std::env::var(username_name).ok(),
        std::env::var(password_name).ok(),
    )
}

impl Uri {
    fn try_from_active_elasticsearch_env() -> Result<Self> {
        tracing::debug!("Creating URI from active ELASTIC_ES_URL");
        let url = std::env::var("ELASTIC_ES_URL").map_err(|_| eyre!("ELASTIC_ES_URL is not defined"))?;
        let (apikey, username, password) =
            try_get_elastic_cli_auth_env("ELASTIC_ES_API_KEY", "ELASTIC_ES_USERNAME", "ELASTIC_ES_PASSWORD");
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .apikey(apikey)
            .username(username)
            .password(password)
            .build()?;
        host.try_into()
    }

    fn try_from_active_kibana_env() -> Result<Self> {
        tracing::debug!("Creating URI from active ELASTIC_KIBANA_URL");
        let url = std::env::var("ELASTIC_KIBANA_URL").map_err(|_| eyre!("ELASTIC_KIBANA_URL is not defined"))?;
        let (apikey, username, password) = try_get_elastic_cli_auth_env(
            "ELASTIC_KIBANA_API_KEY",
            "ELASTIC_KIBANA_USERNAME",
            "ELASTIC_KIBANA_PASSWORD",
        );
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .product(Product::Kibana)
            .apikey(apikey)
            .username(username)
            .password(password)
            .build()?;
        host.try_into()
    }

    fn try_from_active_cloud_env() -> Result<Self> {
        tracing::debug!("Creating URI from active ELASTIC_CLOUD_URL");
        let url = std::env::var("ELASTIC_CLOUD_URL").map_err(|_| eyre!("ELASTIC_CLOUD_URL is not defined"))?;
        let apikey = std::env::var("ELASTIC_CLOUD_API_KEY").ok();
        let host = KnownHostBuilder::new(Url::parse(&url)?).apikey(apikey).build()?;
        host.try_into()
    }

    /// Try creating a new Elasticsearch Uri from the environment variables
    /// - `ESDIAG_OUTPUT_URL` (required): The URL to use for Elasticsearch output.
    /// - `ESDIAG_OUTPUT_APIKEY` (optional): API key for authentication.
    /// - `ESDIAG_OUTPUT_USERNAME` (optional): Username for authentication.
    /// - `ESDIAG_OUTPUT_PASSWORD` (optional): Password for authentication.
    pub fn try_from_output_env() -> Result<Self> {
        tracing::debug!("Creating URI from ESDIAG_OUTPUT_URL or ELASTIC_ES_URL");
        let url = env::get_optional_string_with_fallback("ESDIAG_OUTPUT_URL", "ELASTIC_ES_URL")
            .ok_or_else(|| eyre!("ESDIAG_OUTPUT_URL and ELASTIC_ES_URL are not defined"))?;
        tracing::debug!("output: Env {}", url);
        let (apikey, username, password) =
            try_get_auth_env("ELASTIC_ES_API_KEY", "ELASTIC_ES_USERNAME", "ELASTIC_ES_PASSWORD")?;
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .apikey(apikey)
            .username(username)
            .password(password)
            .build()?;
        host.try_into()
    }

    /// Try creating a new Kibana Uri from the environment variables
    /// - `ESDIAG_KIBANA_URL` (required): The URL to use for Kibana.
    /// - `ESDIAG_OUTPUT_APIKEY` (optional): API key for authentication.
    /// - `ESDIAG_OUTPUT_USERNAME` (optional): Username for authentication.
    /// - `ESDIAG_OUTPUT_PASSWORD` (optional): Password for authentication.
    pub fn try_from_kibana_env() -> Result<Self> {
        tracing::debug!("Creating URI from ESDIAG_KIBANA_URL or ELASTIC_KIBANA_URL");
        let url = env::get_optional_string_with_fallback("ESDIAG_KIBANA_URL", "ELASTIC_KIBANA_URL")
            .ok_or_else(|| eyre!("ESDIAG_KIBANA_URL and ELASTIC_KIBANA_URL are not defined"))?;
        tracing::debug!("kibana: Env {}", url);
        let (apikey, username, password) = try_get_auth_env(
            "ELASTIC_KIBANA_API_KEY",
            "ELASTIC_KIBANA_USERNAME",
            "ELASTIC_KIBANA_PASSWORD",
        )?;
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .product(Product::Kibana)
            .apikey(apikey)
            .username(username)
            .password(password)
            .build()?;
        host.try_into()
    }

    /// Try creating a new Elastic Cloud Uri from the environment variables.
    pub fn try_from_cloud_env() -> Result<Self> {
        tracing::debug!("Creating URI from ESDIAG_CLOUD_URL or ELASTIC_CLOUD_URL");
        let url = env::get_optional_string_with_fallback("ESDIAG_CLOUD_URL", "ELASTIC_CLOUD_URL")
            .ok_or_else(|| eyre!("ESDIAG_CLOUD_URL and ELASTIC_CLOUD_URL are not defined"))?;
        let apikey = env::get_optional_string_with_fallback("ESDIAG_CLOUD_APIKEY", "ELASTIC_CLOUD_API_KEY");
        let host = KnownHostBuilder::new(Url::parse(&url)?).apikey(apikey).build()?;
        host.try_into()
    }
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Uri::try_from(&s).map_err(serde::de::Error::custom)
    }
}

impl From<Uri> for Url {
    fn from(uri: Uri) -> Self {
        match uri {
            Uri::Directory(path) => Url::from_directory_path(path).unwrap(),
            Uri::ElasticCloud(host) => host.into(),
            Uri::ElasticCloudAdmin(host) => host.into(),
            Uri::ElasticGovCloudAdmin(host) => host.into(),
            Uri::File(path) => Url::from_file_path(path).unwrap(),
            Uri::KnownHost(host) => host.into(),
            Uri::ServiceLink(url) => url,
            Uri::ServiceLinkNoAuth(url) => url,
            Uri::Stream => Url::parse("stdin://").unwrap(),
            Uri::Url(url) => url,
        }
    }
}

impl TryFrom<KnownHost> for Uri {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        if host.is_template() {
            return Err(eyre!(
                "Template-backed hosts must be resolved into a concrete URL before runtime use"
            ));
        }
        let host_uri = match host.cloud_id() {
            Some(ElasticCloud::ElasticCloud) => Uri::KnownHost(host),
            Some(ElasticCloud::ElasticCloudAdmin) => Uri::ElasticCloudAdmin(host),
            Some(ElasticCloud::ElasticGovCloudAdmin) => Uri::ElasticGovCloudAdmin(host),
            None => Uri::KnownHost(host),
        };
        Ok(host_uri)
    }
}

impl TryFrom<&str> for Uri {
    type Error = Report;

    fn try_from(uri: &str) -> Result<Self> {
        if uri == "-" || uri == "stdio://stdout" {
            tracing::debug!("Creating Uri::Stream");
            return Ok(Uri::Stream);
        }

        if let Some(service) = parse_active_context_service(uri) {
            tracing::debug!("Creating Uri from active Elastic CLI context reference: {uri}");
            if env::is_elastic_cli_invocation() {
                match try_from_active_context_service(service) {
                    Ok(uri) => return Ok(uri),
                    Err(active_context_error) => {
                        #[cfg(feature = "elasticrc")]
                        if let Some(uri) = try_from_current_elasticrc_reference(uri)? {
                            tracing::debug!("Creating Uri from Elastic CLI current context reference: {uri}");
                            return Ok(uri);
                        }
                        return Err(active_context_error);
                    }
                }
            }

            #[cfg(feature = "elasticrc")]
            if let Some(uri) = try_from_current_elasticrc_reference(uri)? {
                tracing::debug!("Creating Uri from Elastic CLI current context reference: {uri}");
                return Ok(uri);
            }
        }

        #[cfg(feature = "elasticrc")]
        if let Some(uri) = try_from_elasticrc_reference(uri)? {
            tracing::debug!("Creating Uri from Elastic CLI config reference: {uri}");
            return Ok(uri);
        }

        if let Some(host) = KnownHost::resolve_template_reference(uri)? {
            return host.try_into();
        }

        if let Ok(host) = KnownHost::from_str(uri) {
            if host.is_template() {
                return Err(eyre!(KnownHost::template_guidance(uri)));
            }
            return host.try_into();
        }
        tracing::debug!("No known host for {uri}");

        if let Ok(url) = Url::parse(uri) {
            if url.scheme() == "file" {
                let path = url.to_file_path().map_err(|_| eyre!("Invalid file URI: {uri}"))?;
                if uri.ends_with('/') {
                    return Ok(Uri::Directory(path));
                }
                if path.exists() {
                    return if path.is_dir() {
                        Ok(Uri::Directory(path))
                    } else {
                        Ok(Uri::File(path))
                    };
                }
                return Ok(Uri::File(path));
            }
            let domain = url.domain().ok_or_eyre("URL is missing a domain")?;
            match (domain, url.username(), url.password()) {
                ("upload.elastic.co", "token", Some(_)) => {
                    tracing::debug!("Creating Uri::ElasticUploader");
                    return Ok(Uri::ServiceLink(url));
                }
                ("upload.elastic.co", _, None) => {
                    tracing::debug!("Missing auth token for Elastic Uploader");
                    return Ok(Uri::ServiceLinkNoAuth(url));
                }
                _ => {
                    tracing::debug!("Creating Uri::Url");
                    return Ok(Uri::Url(url));
                }
            }
        }

        let path = Path::new(&uri);
        match path.is_dir() {
            false => tracing::debug!("Not an existing directory {uri}"),
            true => {
                tracing::debug!("Directory {uri}");
                let path_buf = PathBuf::from_str(uri)?;
                return Ok(Uri::Directory(path_buf));
            }
        }

        match path.is_file() {
            false => {
                if path.extension().is_none() {
                    tracing::debug!("No extension, creating directory: {uri}");
                    let path_buf = PathBuf::from_str(uri)?;
                    Ok(Uri::Directory(path_buf))
                } else {
                    tracing::debug!("File did not exist: {uri}");
                    Ok(Uri::File(PathBuf::from_str(uri)?))
                }
            }
            true => Ok(Uri::File(PathBuf::from_str(uri)?)),
        }
    }
}

impl TryFrom<&String> for Uri {
    type Error = Report;

    fn try_from(uri: &String) -> Result<Self> {
        Uri::try_from(uri.as_str())
    }
}

impl TryFrom<String> for Uri {
    type Error = Report;

    fn try_from(uri: String) -> Result<Self> {
        Uri::try_from(uri.as_str())
    }
}

impl TryFrom<Option<String>> for Uri {
    type Error = Report;

    fn try_from(uri: Option<String>) -> Result<Self> {
        match uri {
            Some(uri) => Uri::try_from(uri),
            None => Uri::try_from_output_env(),
        }
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Uri::Directory(path) => write!(f, "{}", path.display()),
            Uri::ElasticCloud(host) => write!(f, "{}", host),
            Uri::ElasticCloudAdmin(host) => write!(f, "{}", host),
            Uri::ElasticGovCloudAdmin(host) => write!(f, "{}", host),
            Uri::File(path) => write!(f, "{}", path.display()),
            Uri::KnownHost(host) => write!(f, "{}", host),
            Uri::ServiceLink(url) => {
                write!(f, "{}{}", url.domain().expect("No domain"), url.path())
            }
            Uri::ServiceLinkNoAuth(url) => {
                write!(f, "{}{}", url.domain().expect("No domain"), url.path())
            }
            Uri::Stream => write!(f, "-"),
            Uri::Url(url) => write!(f, "{}", url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Uri;
    use crate::data::{Auth, ElasticCloud, HostRole, KnownHost, Product};
    use std::{collections::BTreeMap, path::Path};
    use tempfile::TempDir;

    const ENV_VARS: &[&str] = &[
        "ESDIAG_OUTPUT_URL",
        "ESDIAG_OUTPUT_APIKEY",
        "ESDIAG_OUTPUT_USERNAME",
        "ESDIAG_OUTPUT_PASSWORD",
        "ESDIAG_KIBANA_URL",
        "ESDIAG_CLOUD_URL",
        "ESDIAG_CLOUD_APIKEY",
        "ELASTIC_ES_URL",
        "ELASTIC_ES_API_KEY",
        "ELASTIC_ES_USERNAME",
        "ELASTIC_ES_PASSWORD",
        "ELASTIC_KIBANA_URL",
        "ELASTIC_KIBANA_API_KEY",
        "ELASTIC_KIBANA_USERNAME",
        "ELASTIC_KIBANA_PASSWORD",
        "ELASTIC_CLOUD_URL",
        "ELASTIC_CLOUD_API_KEY",
        "ESDIAG_ELASTIC_CLI",
        "ESDIAG_HOSTS",
        "ELASTIC_CLI_CONFIG_FILE",
    ];

    fn clear_env() {
        unsafe {
            for name in ENV_VARS {
                std::env::remove_var(name);
            }
        }
    }

    fn setup_hosts_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", tmp.path().join("hosts.yml"));
        }
        tmp
    }

    fn write_elasticrc(contents: &str) -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        std::fs::write(&path, contents).expect("write elasticrc");
        unsafe {
            std::env::set_var("ELASTIC_CLI_CONFIG_FILE", &path);
        }
        tmp
    }

    #[test]
    fn parses_stdio_stdout_uri_as_stream() {
        assert!(matches!(Uri::try_from("stdio://stdout"), Ok(Uri::Stream)));
        assert!(matches!(Uri::try_from("-"), Ok(Uri::Stream)));
    }

    #[test]
    fn parses_file_uri_directory_and_file_targets() {
        assert!(matches!(
            Uri::try_from("file:///tmp/output/"),
            Ok(Uri::Directory(path)) if path == Path::new("/tmp/output")
        ));
        assert!(matches!(
            Uri::try_from("file:///tmp/output/report.ndjson"),
            Ok(Uri::File(path)) if path == Path::new("/tmp/output/report.ndjson")
        ));
        assert!(matches!(
            Uri::try_from("file:///tmp/REPORT"),
            Ok(Uri::File(path)) if path == Path::new("/tmp/REPORT")
        ));
    }

    #[test]
    fn output_env_uses_elastic_api_key_fallback() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ELASTIC_ES_URL", "https://elastic.example:9200");
            std::env::set_var("ELASTIC_ES_API_KEY", "elastic-key");
        }

        let Uri::KnownHost(host) = Uri::try_from_output_env().expect("output env uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://elastic.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "elastic-key"));
        clear_env();
    }

    #[test]
    fn output_env_uses_elastic_basic_fallback() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ELASTIC_ES_URL", "https://elastic.example:9200");
            std::env::set_var("ELASTIC_ES_USERNAME", "elastic");
            std::env::set_var("ELASTIC_ES_PASSWORD", "changeme");
        }

        let Uri::KnownHost(host) = Uri::try_from_output_env().expect("output env uri") else {
            panic!("expected known host");
        };

        assert!(matches!(
            host.get_auth().expect("auth"),
            Auth::Basic(user, password) if user == "elastic" && password == "changeme"
        ));
        clear_env();
    }

    #[test]
    fn output_env_prefers_esdiag_values_over_elastic_fallbacks() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "https://esdiag.example:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "esdiag-key");
            std::env::set_var("ELASTIC_ES_URL", "https://elastic.example:9200");
            std::env::set_var("ELASTIC_ES_API_KEY", "elastic-key");
        }

        let Uri::KnownHost(host) = Uri::try_from_output_env().expect("output env uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://esdiag.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "esdiag-key"));
        clear_env();
    }

    #[test]
    fn kibana_env_uses_elastic_kibana_fallbacks() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ELASTIC_KIBANA_URL", "https://kibana.example:5601");
            std::env::set_var("ELASTIC_KIBANA_USERNAME", "elastic");
            std::env::set_var("ELASTIC_KIBANA_PASSWORD", "changeme");
        }

        let Uri::KnownHost(host) = Uri::try_from_kibana_env().expect("kibana env uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.app(), &Product::Kibana);
        assert_eq!(host.get_url().expect("url").as_str(), "https://kibana.example:5601/");
        assert!(matches!(
            host.get_auth().expect("auth"),
            Auth::Basic(user, password) if user == "elastic" && password == "changeme"
        ));
        clear_env();
    }

    #[test]
    fn cloud_env_uses_elastic_cloud_api_key_path() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var(
                "ELASTIC_CLOUD_URL",
                "https://cloud.elastic.co/deployments/deployment-123",
            );
            std::env::set_var("ELASTIC_CLOUD_API_KEY", "cloud-key");
        }

        let Uri::KnownHost(host) = Uri::try_from_cloud_env().expect("cloud env uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.cloud_id(), Some(&ElasticCloud::ElasticCloud));
        assert_eq!(
            host.get_url().expect("url").as_str(),
            "https://cloud.elastic.co/api/v1/deployments/deployment-123/elasticsearch/_main/proxy/"
        );
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "cloud-key"));
        clear_env();
    }

    #[test]
    fn active_context_es_alias_resolves_before_saved_hosts() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = setup_hosts_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            ".es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                url::Url::parse("https://saved.example:9200").expect("saved url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        unsafe {
            std::env::set_var("ESDIAG_ELASTIC_CLI", "1");
            std::env::set_var("ELASTIC_ES_URL", "https://active.example:9200");
            std::env::set_var("ELASTIC_ES_API_KEY", "active-key");
        }

        let Uri::KnownHost(host) = Uri::try_from(".es").expect("active context uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://active.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "active-key"));
        clear_env();
    }

    #[test]
    fn active_context_es_alias_prefers_elastic_context_over_esdiag_output() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_ELASTIC_CLI", "1");
            std::env::set_var("ESDIAG_OUTPUT_URL", "https://output.example:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "output-key");
            std::env::set_var("ELASTIC_ES_URL", "https://active.example:9200");
            std::env::set_var("ELASTIC_ES_API_KEY", "active-key");
        }

        let Uri::KnownHost(host) = Uri::try_from(".es").expect("active context uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://active.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "active-key"));
        clear_env();
    }

    #[test]
    fn standalone_active_context_alias_falls_through_to_saved_host() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let previous_home = std::env::var_os("HOME");
        let _tmp = setup_hosts_env();
        let home = TempDir::new().expect("home temp dir");
        let mut hosts = BTreeMap::new();
        hosts.insert(
            ".es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                url::Url::parse("https://saved.example:9200").expect("saved url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::set_var("ELASTIC_ES_URL", "https://active.example:9200");
        }

        let Uri::KnownHost(host) = Uri::try_from(".es").expect("saved host uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://saved.example:9200/");
        clear_env();
        unsafe {
            if let Some(previous_home) = previous_home {
                std::env::set_var("HOME", previous_home);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    #[test]
    fn active_context_kibana_alias_resolves() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_ELASTIC_CLI", "1");
            std::env::set_var("ELASTIC_KIBANA_URL", "https://active-kb.example:5601");
        }

        let Uri::KnownHost(host) = Uri::try_from(".kb").expect("active context uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.app(), &Product::Kibana);
        assert_eq!(host.get_url().expect("url").as_str(), "https://active-kb.example:5601/");
        clear_env();
    }

    #[test]
    fn active_context_cloud_service_resolves() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_ELASTIC_CLI", "1");
            std::env::set_var(
                "ELASTIC_CLOUD_URL",
                "https://cloud.elastic.co/deployments/deployment-123",
            );
            std::env::set_var("ELASTIC_CLOUD_API_KEY", "cloud-key");
        }

        let Uri::KnownHost(host) = Uri::try_from(".cloud").expect("active context uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.cloud_id(), Some(&ElasticCloud::ElasticCloud));
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "cloud-key"));
        clear_env();
    }

    #[test]
    fn non_service_leading_dot_falls_through_to_path_resolution() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();

        assert!(matches!(
            Uri::try_from(".unknown"),
            Ok(Uri::Directory(path)) if path == Path::new(".unknown")
        ));
        clear_env();
    }

    #[test]
    fn explicit_hidden_local_path_bypasses_active_context_reference() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ELASTIC_ES_URL", "https://active.example:9200");
        }

        assert!(matches!(
            Uri::try_from("./.es"),
            Ok(Uri::Directory(path)) if path == Path::new("./.es")
        ));
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_resolves_elasticsearch() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
      auth:
        api_key: prod-key
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".prod.es").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://prod.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "prod-key"));
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn active_elasticrc_reference_resolves_current_context() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
      auth:
        api_key: prod-key
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".es").expect("elasticrc current context uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://prod.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "prod-key"));
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_resolves_canonical_elasticsearch() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".prod.elasticsearch").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://prod.example:9200/");
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_resolves_dotted_context_name() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod.us-west
contexts:
  prod.us-west:
    elasticsearch:
      url: https://west.example:9200
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".prod.us-west.es").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://west.example:9200/");
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_resolves_kibana() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    kibana:
      url: https://kb.example:5601
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".prod.kb").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.app(), &Product::Kibana);
        assert_eq!(host.get_url().expect("url").as_str(), "https://kb.example:5601/");
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_resolves_cloud() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    cloud:
      url: https://cloud.elastic.co/deployments/deployment-123
      auth:
        api_key: cloud-key
"#,
        );

        let Uri::KnownHost(host) = Uri::try_from(".prod.cloud").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.cloud_id(), Some(&ElasticCloud::ElasticCloud));
        assert_eq!(
            host.get_url().expect("url").as_str(),
            "https://cloud.elastic.co/api/v1/deployments/deployment-123/elasticsearch/_main/proxy/"
        );
        assert!(matches!(host.get_auth().expect("auth"), Auth::Apikey(key) if key == "cloud-key"));
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_takes_precedence_over_saved_host() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _hosts_tmp = setup_hosts_env();
        let _elasticrc_tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://config.example:9200
"#,
        );
        let mut hosts = BTreeMap::new();
        hosts.insert(
            ".prod.es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                url::Url::parse("https://saved.example:9200").expect("saved url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");

        let Uri::KnownHost(host) = Uri::try_from(".prod.es").expect("elasticrc uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://config.example:9200/");
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn named_elasticrc_reference_reports_missing_context() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
"#,
        );

        let err = match Uri::try_from(".diag.es") {
            Ok(_) => panic!("missing context should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("Elastic CLI context 'diag' was not found"));
        clear_env();
    }

    #[cfg(feature = "elasticrc")]
    #[test]
    fn unsupported_named_service_falls_through_to_path_resolution() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        let _tmp = write_elasticrc(
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://prod.example:9200
"#,
        );

        assert!(matches!(
            Uri::try_from(".prod.ls"),
            Ok(Uri::File(path)) if path == Path::new(".prod.ls")
        ));
        clear_env();
    }
}
