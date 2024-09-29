use crate::host::Host;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;

/// Represents various types of URIs classified by the system.
#[derive(Debug)]
pub enum Uri {
    /// Represents a host saved in the hosts.yml
    Host(Host),
    /// Represents a standard URL
    Url(Url),
    /// Represents a directory path on the local file system
    Directory(PathBuf),
    /// Represents a file path on the local filesystem
    File(PathBuf),
    /// Represents an input/output stream (e.g., stdin/stdout)
    Stream,
}

/// Classifies a URI string into a specific `Uri` variant based on its type.
///
/// This function takes a URI string and categorizes it into different types represented by the `Uri` enum.
/// It supports classifying a URI as a stream, host, URL, directory, or file based on various checks.
///
/// # Arguments
///
/// * `uri` - A string slice representing the URI to classify.
///
/// # Returns
///
/// Returns a `Result` with a `Uri` enum variant:
/// - `Ok(Uri::Stream)` if the URI is `"-"`.
/// - `Ok(Uri::Host(host))` if the URI can be parsed into a `Host`.
/// - `Ok(Uri::Url(url))` if the URI can be parsed into a `Url`.
/// - `Ok(Uri::Directory(path))` if the URI is a valid directory path.
/// - `Ok(Uri::File(path))` if the URI is a valid file path.
/// - `Err(std::io::Error)` if there are errors during file creation or other I/O operations.
///
/// # Errors
///
/// Returns an `Err(std::io::Error)` if there are errors during file creation or other I/O operations.
///
/// # Examples
///
/// ```rust
/// use std::path::PathBuf;
///
/// let uri = "-";
/// match classify(uri) {
///     Ok(Uri::Stream) => println!("URI is a stream"),
///     Ok(_) => println!("URI classified successfully"),
///     Err(e) => eprintln!("Failed to classify URI: {}", e),
/// }
/// ```

impl Uri {
    pub fn parse(uri: &str) -> Result<Self, std::io::Error> {
        match uri {
            "-" => Ok(Uri::Stream),
            _ => {
                let host = Host::from_str(&uri);
                match host {
                    Err(_) => log::debug!("No known host {uri}"),
                    Ok(host) => return Ok(Uri::Host(host)),
                }
                match Url::parse(&uri) {
                    Err(_) => log::debug!("Not a valid URL {uri}"),
                    Ok(url) => return Ok(Uri::Url(url)),
                }
                let path = Path::new(&uri);
                match path.is_dir() {
                    false => log::debug!("Not a directory {uri}"),
                    true => {
                        log::debug!("Directory {uri}");
                        let mut path_buf = PathBuf::from_str(&uri).unwrap();
                        // Push manifest.json so we can later replace with `.with_file_name()`
                        path_buf.push("manifest.json");
                        return Ok(Uri::Directory(path_buf));
                    }
                }
                match path.is_file() {
                    false => log::debug!("Not a file {uri}"),
                    true => return Ok(Uri::File(PathBuf::from_str(&uri).unwrap())),
                }
                match std::fs::File::create(&uri) {
                    Ok(_) => {
                        log::info!("No existing output target, created file {uri}");
                        Ok(Uri::File(
                            PathBuf::from_str(&uri).expect("Failed to create file"),
                        ))
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Uri::Host(host) => write!(f, "{}", host),
            Uri::Url(url) => write!(f, "{}", url),
            Uri::Directory(path) => write!(f, "{}", path.display()),
            Uri::File(path) => write!(f, "{}", path.display()),
            Uri::Stream => write!(f, "-"),
        }
    }
}
