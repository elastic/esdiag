use crate::host::Host;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;

#[derive(Debug)]
pub enum Uri {
    Host(Host),
    Url(Url),
    Directory(PathBuf),
    File(PathBuf),
    Stream,
}

pub fn classify(uri: &str) -> Result<Uri, std::io::Error> {
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
            match Path::new(&uri).is_dir() {
                false => log::debug!("Not a directory {uri}"),
                true => {
                    log::debug!("Directory {uri}");
                    return Ok(Uri::Directory(PathBuf::from_str(&uri).unwrap()));
                }
            }
            match Path::new(&uri).is_file() {
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
