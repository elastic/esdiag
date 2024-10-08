use super::Receive;
use crate::data::{diagnostic::data_source::DataSource, Uri};
use color_eyre::{eyre::eyre, Result};
use serde::de::DeserializeOwned;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub struct ArchiveReceiver {
    path: PathBuf,
    uri: Uri,
}

impl TryFrom<Uri> for ArchiveReceiver {
    type Error = color_eyre::eyre::Report;

    fn try_from(uri: Uri) -> Result<Self> {
        match uri {
            Uri::File(ref path) => match path.is_file() {
                true => {
                    log::debug!("File is valid: {}", path.display());
                    Ok(Self {
                        path: path.clone(),
                        uri,
                    })
                }
                false => {
                    log::debug!("File is invalid: {}", path.display());
                    Err(eyre!("Archive input must be a file: {}", path.display()))
                }
            },
            _ => Err(eyre!("Input must be a file")),
        }
    }
}

impl Receive for ArchiveReceiver {
    async fn is_connected(&self) -> bool {
        let is_file = self.path.is_file();
        let filename = self.path.to_str().unwrap_or("");
        log::debug!("Directory {filename} is valid: {is_file}");
        is_file
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let mut archive = zip::ZipArchive::new(File::open(self.path.as_path())?)?;
        let filename = T::source(&self.uri)?;

        // Use the first file in the archive to determine the path
        let file_path = {
            let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
            if path.extension() != None {
                path.pop();
            }
            path.push(filename);
            path.to_str()
                .expect("Archive PathBuf to string failed")
                .to_string()
        };

        // Read lines directly from the compressed file
        log::debug!("Reading {}", file_path);
        let file = archive.by_name(&file_path)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl std::fmt::Display for ArchiveReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

// Old implementation

pub fn read_string(
    archive_path: &PathBuf,
    filename: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;

    // Use the first file in the archive to determine the path
    let file_path = {
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        if path.extension() != None {
            path.pop();
        }
        path.push(filename);
        path.to_str()
            .expect("Archive PathBuf to string failed")
            .to_string()
    };

    // Read lines directly from the compressed file
    log::debug!("From archive {:?}, file \"{}\"", archive_path, file_path);
    let file = archive.by_name(&file_path)?;
    let read_lines = BufReader::new(file).lines();
    let string = read_lines.filter_map(Result::ok).collect::<String>();
    Ok(string)
}
