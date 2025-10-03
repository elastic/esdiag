// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::client::KnownHost;
use crate::env;
use eyre::Result;
use eyre::{OptionExt, Report};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;

/// Save an arbitrary serializable object to a file
pub fn save_file<T: Serialize>(filename: &str, content: &T) -> Result<()> {
    let home_file = PathBuf::from(env::get_string("HOME")?)
        .join(env::get_string("ESDIAG_HOME")?)
        .join("last_run")
        .join(filename);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(home_file)?;
    let body = serde_json::to_string(&content)?;
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

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
        use crate::client::ElasticCloud;
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

/// The standard deserializer from serde_json does not deserializing u64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.

pub fn u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_u64()),
        Value::String(s) => Ok(s.parse::<u64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

/// The standard deserializer from serde_json does not deserializing i64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.

pub fn i64_from_string<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_i64()),
        Value::String(s) => Ok(s.parse::<i64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

pub fn map_as_vec_entries<'de, D, T>(deserializer: D) -> Result<Vec<(String, T)>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Object(map) => {
            let mut result = Vec::new();
            for (key, value) in map {
                let deserialized_value = T::deserialize(value).map_err(serde::de::Error::custom)?;
                result.push((key, deserialized_value));
            }
            Ok(result)
        }
        _ => Err(serde::de::Error::custom("expected an object")),
    }
}

pub fn option_map_as_vec_entries<'de, D, T>(
    deserializer: D,
) -> Result<Option<Vec<(String, T)>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // Deserialize into an Option<Value> so we can distinguish missing / null
    let opt_value: Option<Value> = Option::deserialize(deserializer)?;

    match opt_value {
        None => Ok(None),
        Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => {
            let mut result = Vec::with_capacity(map.len());
            for (key, value) in map {
                let deserialized_value = T::deserialize(value).map_err(serde::de::Error::custom)?;
                result.push((key, deserialized_value));
            }
            Ok(Some(result))
        }
        Some(_) => Err(serde::de::Error::custom(
            "expected an object, null, or missing field",
        )),
    }
}
