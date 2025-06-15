use super::{Receive, ReceiveMultiple, ReceiveRaw};
use crate::data::diagnostic::{DataSource, data_source::PathType};
use eyre::{Result, eyre};
use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
    time::SystemTime,
};

#[derive(Clone)]
pub struct DirectoryReceiver {
    path: PathBuf,
    work_dir: String,
    modified_date: SystemTime,
}

impl TryFrom<PathBuf> for DirectoryReceiver {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        match path.is_dir() {
            true => {
                log::debug!("Directory is valid: {}", path.display());
                Ok(Self {
                    path: path.clone(),
                    work_dir: String::from(""),
                    modified_date: path.metadata()?.modified()?,
                })
            }
            false => {
                log::debug!("Directory is invalid: {}", path.display());
                Err(eyre!(
                    "Directory input must be a directory: {}",
                    path.display()
                ))
            }
        }
    }
}

impl Receive for DirectoryReceiver {
    async fn collection_date(&self) -> String {
        chrono::DateTime::<chrono::Utc>::from(self.modified_date).to_rfc3339()
    }

    async fn is_connected(&self) -> bool {
        let is_dir = self.path.is_dir();
        let directory_name = self.path.to_str().unwrap_or("");
        log::debug!("Directory {directory_name} is valid: {is_dir}");
        is_dir
    }

    async fn get<T>(&self) -> Result<T>
    where
        T: DeserializeOwned + DataSource,
    {
        let filename = &self
            .path
            .join(&self.work_dir)
            .join(T::source(PathType::File)?);
        log::debug!("Reading file: {}", &filename.display());
        let file = File::open(&filename)?;
        let reader = BufReader::new(file);
        let data: T = serde_json::from_reader(reader)?;
        Ok(data)
    }
}

impl ReceiveRaw for DirectoryReceiver {
    async fn get_raw<T>(&self) -> Result<String>
    where
        T: DataSource,
    {
        let filename = &self
            .path
            .join(&self.work_dir)
            .join(T::source(PathType::File)?);
        log::debug!("Reading file: {}", &filename.display());
        let file = File::open(&filename)?;
        let mut reader = BufReader::new(file);
        let mut data = String::new();
        reader.read_to_string(&mut data)?;
        Ok(data)
    }
}

impl ReceiveMultiple for DirectoryReceiver {
    fn set_work_dir(&mut self, work_dir: &str) -> Result<()> {
        self.work_dir = String::from(work_dir);
        Ok(())
    }
}

impl std::fmt::Display for DirectoryReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}
