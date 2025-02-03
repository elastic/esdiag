use crate::client::KnownHost;
use color_eyre::eyre::{eyre, OptionExt, Report, Result};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;

/// The different types of supported URIs
#[derive(Clone, Debug)]
pub enum Uri {
    /// Known host saved in the ~/.esdiag/hosts.yml by default
    KnownHost(KnownHost),
    /// An Elastic Uploader service URL, embed the auth token as `token:<value>@` instead of `username:password` in the URL
    ElasticUploader(Url),
    /// A standard URL
    Url(Url),
    /// Directory on the local file system
    Directory(PathBuf),
    /// File on the local filesystem
    File(PathBuf),
    /// An input/output stream (stdin/stdout)
    Stream,
}

impl TryFrom<&str> for Uri {
    type Error = Report;

    fn try_from(uri: &str) -> Result<Self> {
        if uri == "-" {
            log::debug!("Creating Uri::Stream");
            return Ok(Uri::Stream);
        }

        if let Ok(host) = KnownHost::from_str(&uri) {
            log::debug!("Creating Uri::KnownHost");
            return Ok(Uri::KnownHost(host));
        }
        log::debug!("No known host {uri}");

        if let Ok(url) = Url::parse(&uri) {
            let domain = url.domain().ok_or_eyre("URL is missing a domain")?;
            match (domain, url.username(), url.password()) {
                ("upload.elastic.co", "token", Some(_)) => {
                    log::debug!("Creating Uri::ElasticUploader");
                    return Ok(Uri::ElasticUploader(url));
                }
                ("upload.elastic.co", _, None) => {
                    log::debug!("Missing auth token for Elastic Uploader");
                    return Err(eyre!("Elastic Uploader URLs require an auth token"));
                }
                _ => {
                    log::debug!("Creating Uri::Url");
                    return Ok(Uri::Url(url));
                }
            }
        }

        let path = Path::new(&uri);
        match path.is_dir() {
            false => log::debug!("Not a directory {uri}"),
            true => {
                log::debug!("Directory {uri}");
                let path_buf = PathBuf::from_str(&uri)?;
                return Ok(Uri::Directory(path_buf));
            }
        }
        match path.is_file() {
            false => {
                log::debug!("File does not exist: {uri}");
                return Ok(Uri::File(PathBuf::from_str(&uri)?));
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
            Uri::KnownHost(host) => write!(f, "{}", host),
            Uri::ElasticUploader(url) => {
                write!(f, "{}{}", url.domain().expect("No domain"), url.path())
            }
            Uri::Url(url) => write!(f, "{}", url),
            Uri::Directory(path) => write!(f, "{}", path.display()),
            Uri::File(path) => write!(f, "{}", path.display()),
            Uri::Stream => write!(f, "-"),
        }
    }
}
