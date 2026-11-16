// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::output_deployment::environment_auth;
use super::{
    Application, ElasticCloud, HostReference, KnownHost, KnownHostBuilder, OutputDeployment, ResolvedKnownHost,
};
use eyre::{OptionExt, Report, Result, eyre};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use url::Url;
/// The different types of supported URIs
#[derive(Clone, Debug, Default)]
pub enum Uri {
    /// Known host saved in the ~/.esdiag/hosts.yml by default
    KnownHost(KnownHost),
    /// An Elastic Cloud URL for the Elasticsearch API proxy
    ElasticCloud(KnownHost),
    /// An Elastic Cloud Admin URL for the Elasticsearch API proxy
    ElasticCloudAdmin(KnownHost),
    /// An Elastic Cloud GovCloud Admin URL for the Elasticsearch API proxy
    ElasticGovCloudAdmin(KnownHost),
    /// An Elastic Upload Service URL; embed the auth token as `token:<value>@` instead of `username:password`
    ServiceLink(Url),
    /// An Elastic Upload Service URL without authentication
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

impl Uri {
    /// Try creating a new Elasticsearch Uri from the environment variables
    /// - `ESDIAG_OUTPUT_URL` (required): The URL to use for Elasticsearch output.
    /// - `ESDIAG_OUTPUT_APIKEY` (optional): API key for authentication.
    /// - `ESDIAG_OUTPUT_USERNAME` (optional): Username for authentication.
    /// - `ESDIAG_OUTPUT_PASSWORD` (optional): Password for authentication.
    /// - `ESDIAG_KIBANA_URL` (optional): The output's Kibana viewer.
    pub fn try_from_output_env() -> Result<Self> {
        OutputDeployment::from_environment(false)?.elasticsearch.try_into()
    }

    /// Resolve the default output deployment without mixing runtime and saved
    /// configuration sources. Explicit command targets continue to take
    /// precedence through [`TryFrom<Option<String>>`].
    pub fn try_from_default_output() -> Result<Self> {
        OutputDeployment::resolve(None, false)?.elasticsearch.try_into()
    }

    /// Try creating a new Kibana Uri from the environment variables
    /// - `ESDIAG_KIBANA_URL` (required): The URL to use for Kibana.
    /// - `ESDIAG_OUTPUT_APIKEY` (optional): API key for authentication.
    /// - `ESDIAG_OUTPUT_USERNAME` (optional): Username for authentication.
    /// - `ESDIAG_OUTPUT_PASSWORD` (optional): Password for authentication.
    pub fn try_from_kibana_env() -> Result<Self> {
        let url = std::env::var("ESDIAG_KIBANA_URL").map_err(|_| eyre!("ESDIAG_KIBANA_URL is not defined"))?;
        tracing::debug!("kibana: Env {}", url);
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .application(Application::Kibana)
            .auth(environment_auth()?)
            .build()?;
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

impl Serialize for Uri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Uri::ServiceLink(url) | Uri::ServiceLinkNoAuth(url) | Uri::Url(url) => {
                serializer.serialize_str(redacted_url_string(url).as_str())
            }
            Uri::Directory(path) | Uri::File(path) => serializer.serialize_str(&path.display().to_string()),
            Uri::ElasticCloud(host)
            | Uri::ElasticCloudAdmin(host)
            | Uri::ElasticGovCloudAdmin(host)
            | Uri::KnownHost(host) => Err(serde::ser::Error::custom(format!(
                "resolved host URI '{}' cannot be serialized without losing saved-host semantics",
                host.transport_display()
            ))),
            Uri::Stream => serializer.serialize_str("-"),
        }
    }
}

fn redacted_url_string(url: &Url) -> String {
    let mut url = url.clone();
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.to_string()
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
        host.resolve()?.try_into()
    }
}

impl TryFrom<ResolvedKnownHost> for Uri {
    type Error = eyre::Report;

    fn try_from(host: ResolvedKnownHost) -> Result<Self> {
        let host = host.into_known_host();
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

        if let Some(reference) = HostReference::parse(uri)? {
            return reference.resolve()?.try_into();
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
                    tracing::debug!("Creating authenticated Elastic Upload Service URI");
                    return Ok(Uri::ServiceLink(url));
                }
                ("upload.elastic.co", _, None) => {
                    tracing::debug!("Missing auth token for Elastic Upload Service");
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
            None => Uri::try_from_default_output(),
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
    use crate::data::{Application, Auth};
    use std::path::Path;

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
    fn output_env_rejects_mixed_esdiag_auth_values() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "https://esdiag.example:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "esdiag-key");
            std::env::set_var("ESDIAG_OUTPUT_USERNAME", "elastic");
            std::env::set_var("ESDIAG_OUTPUT_PASSWORD", "changeme");
        }

        let err = match Uri::try_from_output_env() {
            Ok(_) => panic!("mixed auth should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("ESDIAG_OUTPUT_APIKEY cannot be combined"));
        clear_env();
    }

    #[test]
    fn output_env_ignores_elastic_cli_variables() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "https://esdiag.example:9200");
            std::env::set_var("ELASTIC_ES_URL", "https://active-context.example:9200");
            std::env::set_var("ELASTIC_ES_API_KEY", "active-context-key");
            std::env::set_var("ELASTIC_KIBANA_URL", "https://active-context.example:5601");
        }

        let Uri::KnownHost(host) = Uri::try_from_output_env().expect("output env uri") else {
            panic!("expected known host");
        };

        assert_eq!(host.get_url().expect("url").as_str(), "https://esdiag.example:9200/");
        assert!(matches!(host.get_auth().expect("auth"), Auth::None));
        assert!(host.attached_viewer().is_none());
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
        }
        assert!(Uri::try_from_output_env().is_err(), "ELASTIC_ES_URL is not an output");
        assert!(
            Uri::try_from_kibana_env().is_err(),
            "ELASTIC_KIBANA_URL is not a viewer"
        );
        clear_env();
    }

    #[test]
    fn output_env_attaches_its_kibana_viewer() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        clear_env();
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "https://esdiag.example:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "esdiag-key");
            std::env::set_var("ESDIAG_KIBANA_URL", "https://esdiag.example:5601");
        }

        let Uri::KnownHost(host) = Uri::try_from_output_env().expect("output env uri") else {
            panic!("expected known host");
        };

        let viewer = host.attached_viewer().expect("environment viewer");
        assert_eq!(viewer.app(), Some(Application::Kibana));
        assert_eq!(viewer.get_url().expect("url").as_str(), "https://esdiag.example:5601/");
        assert!(matches!(viewer.get_auth().expect("auth"), Auth::Apikey(key) if key.expose_secret() == "esdiag-key"));
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
}
