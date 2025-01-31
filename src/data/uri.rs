use crate::client::KnownHost;
use color_eyre::eyre::{eyre, OptionExt, Report, Result};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;

/// Represents various types of URIs classified by the system.
#[derive(Clone, Debug)]
pub enum Uri {
    /// Represents a host saved in the hosts.yml
    KnownHost(KnownHost),
    /// Represents an Elastic Uploader service URL
    ElasticUploader(Url),
    /// Represents a standard URL
    Url(Url),
    /// Represents a directory path on the local file system
    Directory(PathBuf),
    /// Represents a file path on the local filesystem
    File(PathBuf),
    /// Represents an input/output stream (e.g., stdin/stdout)
    Stream,
}

/// Converts a string slice into a specific `Uri` variant based on its type.
///
/// This implementation of the `TryFrom` trait takes a URI string and categorizes it into different types represented by the `Uri` enum.
/// It supports converting a URI string into a stream, host, URL, directory, or file based on various checks.
///
/// # Arguments
///
/// * `uri` - A string slice representing the URI to convert.
///
/// # Returns
///
/// Returns a `Result` with a `Uri` enum variant:
/// - `Ok(Uri::Stream)` if the URI is `"-"`.
/// - `Ok(Uri::Host(host))` if the URI can be parsed into a `Host`.
/// - `Ok(Uri::Url(url))` if the URI can be parsed into a `Url`.
/// - `Ok(Uri::Directory(path))` if the URI is a valid directory path.
/// - `Ok(Uri::File(path))` if the URI is a valid file path.
/// - `Err(Report)` if there are errors during file creation or other I/O operations.
///
/// # Errors
///
/// Returns an `Err(Report)` if there are errors during file creation or other I/O operations.
///
/// # Examples
///
/// ```rust
/// use std::path::PathBuf;
///
/// let uri = "-";
/// match Uri::try_from(uri) {
///     Ok(Uri::Stream) => println!("URI is a stream"),
///     Ok(_) => println!("URI classified successfully"),
///     Err(e) => eprintln!("Failed to parse URI: {}", e),
/// }
/// ```

impl TryFrom<&str> for Uri {
    type Error = Report;

    fn try_from(uri: &str) -> Result<Self> {
        match uri {
            "-" => Ok(Uri::Stream),
            _ => {
                let host = KnownHost::from_str(&uri);
                match host {
                    Err(_) => log::debug!("No known host {uri}"),
                    Ok(host) => return Ok(Uri::KnownHost(host)),
                }
                match Url::parse(&uri) {
                    Err(_) => log::error!("Not a valid URL {uri}"),
                    Ok(url) => {
                        let domain = url.domain().ok_or_eyre("URL is missing a domain")?;
                        log::debug!(
                            "Domain: {domain} Username: {} Password: {}",
                            url.username(),
                            url.password().is_some()
                        );
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
                }
                let path = Path::new(&uri);
                match path.is_dir() {
                    false => log::debug!("Not a directory {uri}"),
                    true => {
                        log::debug!("Directory {uri}");
                        let path_buf = PathBuf::from_str(&uri).unwrap();
                        return Ok(Uri::Directory(path_buf));
                    }
                }
                match path.is_file() {
                    false => {
                        log::debug!("File does not exist: {uri}");
                        return Ok(Uri::File(PathBuf::from_str(&uri).unwrap()));
                    }
                    true => return Ok(Uri::File(PathBuf::from_str(&uri).unwrap())),
                }
            }
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
            Uri::ElasticUploader(url) => write!(f, "{}", url),
            Uri::Url(url) => write!(f, "{}", url),
            Uri::Directory(path) => write!(f, "{}", path.display()),
            Uri::File(path) => write!(f, "{}", path.display()),
            Uri::Stream => write!(f, "-"),
        }
    }
}
