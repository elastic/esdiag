// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::Product;

use super::{ElasticCloud, KnownHost, KnownHostBuilder};
use eyre::{OptionExt, Report, Result, eyre};
use serde::{Deserialize, Deserializer};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use url::Url;
/// The different types of supported URIs
#[derive(Clone)]
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
    Stream,
}

/// Try reading the authentication environment variables.
/// Returns a tuple of optional strings for (apikey, username, password)
fn try_get_auth_env() -> Result<(Option<String>, Option<String>, Option<String>)> {
    let apikey = std::env::var("ESDIAG_OUTPUT_APIKEY").ok();
    let username = std::env::var("ESDIAG_OUTPUT_USERNAME").ok();
    let password = std::env::var("ESDIAG_OUTPUT_PASSWORD").ok();
    Ok((apikey, username, password))
}

impl Uri {
    /// Try creating a new Elasticsearch Uri from the environment variables
    /// - `ESDIAG_OUTPUT_URL` (required): The URL to use for Elasticsearch output.
    /// - `ESDIAG_OUTPUT_APIKEY` (optional): API key for authentication.
    /// - `ESDIAG_OUTPUT_USERNAME` (optional): Username for authentication.
    /// - `ESDIAG_OUTPUT_PASSWORD` (optional): Password for authentication.
    pub fn try_from_output_env() -> Result<Self> {
        log::debug!("Creating URI from ESDIAG_OUTPUT_URL");
        let url = std::env::var("ESDIAG_OUTPUT_URL")
            .map_err(|_| eyre!("ESDIAG_OUTPUT_URL is not defined"))?;
        log::debug!("output: Env {}", url);
        let (apikey, username, password) = try_get_auth_env()?;
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
        log::debug!("Creating URI from ESDIAG_KIBANA_URL");
        let url = std::env::var("ESDIAG_KIBANA_URL")
            .map_err(|_| eyre!("ESDIAG_KIBANA_URL is not defined"))?;
        log::debug!("kibana: Env {}", url);
        let (apikey, username, password) = try_get_auth_env()?;
        let host = KnownHostBuilder::new(Url::parse(&url)?)
            .product(Product::Kibana)
            .apikey(apikey)
            .username(username)
            .password(password)
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

impl Default for Uri {
    fn default() -> Self {
        Uri::Stream
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
        let host_uri = match host {
            KnownHost::ApiKey { ref cloud_id, .. } => match cloud_id {
                Some(ElasticCloud::ElasticCloud) => Uri::ElasticCloud(host),
                Some(ElasticCloud::ElasticCloudAdmin) => Uri::ElasticCloudAdmin(host),
                Some(ElasticCloud::ElasticGovCloudAdmin) => Uri::ElasticGovCloudAdmin(host),
                None => Uri::KnownHost(host),
            },
            KnownHost::Basic { .. } => Uri::KnownHost(host),
            KnownHost::NoAuth { .. } => Uri::KnownHost(host),
        };
        Ok(host_uri)
    }
}

impl TryFrom<&str> for Uri {
    type Error = Report;

    fn try_from(uri: &str) -> Result<Self> {
        if uri == "-" {
            log::debug!("Creating Uri::Stream");
            return Ok(Uri::Stream);
        }

        if let Ok(host) = KnownHost::from_str(&uri) {
            return host.try_into();
        }
        log::debug!("No known host for {uri}");

        if let Ok(url) = Url::parse(&uri) {
            let domain = url.domain().ok_or_eyre("URL is missing a domain")?;
            match (domain, url.username(), url.password()) {
                ("upload.elastic.co", "token", Some(_)) => {
                    log::debug!("Creating Uri::ElasticUploader");
                    return Ok(Uri::ServiceLink(url));
                }
                ("upload.elastic.co", _, None) => {
                    log::debug!("Missing auth token for Elastic Uploader");
                    return Ok(Uri::ServiceLinkNoAuth(url));
                }
                _ => {
                    log::debug!("Creating Uri::Url");
                    return Ok(Uri::Url(url));
                }
            }
        }

        let path = Path::new(&uri);
        match path.is_dir() {
            false => log::debug!("Not an existing directory {uri}"),
            true => {
                log::debug!("Directory {uri}");
                let path_buf = PathBuf::from_str(&uri)?;
                return Ok(Uri::Directory(path_buf));
            }
        }

        match path.is_file() {
            false => {
                if path.extension().is_none() {
                    log::debug!("No extension, creating directory: {uri}");
                    let path_buf = PathBuf::from_str(&uri)?;
                    return Ok(Uri::Directory(path_buf));
                } else {
                    log::debug!("File did not exist: {uri}");
                    return Ok(Uri::File(PathBuf::from_str(&uri)?));
                }
            }
            true => return Ok(Uri::File(PathBuf::from_str(&uri)?)),
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
