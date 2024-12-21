use crate::data::Uri;
use color_eyre::eyre::{eyre, Result};
use std::{fs::File, io::Write, path::PathBuf};

pub struct DirectoryExporter {
    path: PathBuf,
}

impl DirectoryExporter {
    pub async fn save(&self, path: PathBuf, content: String) -> Result<()> {
        let path = &self.path.join(path);
        log::debug!("Writing file: {}", &path.display());
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}

impl TryFrom<Uri> for DirectoryExporter {
    type Error = color_eyre::eyre::Report;

    fn try_from(uri: Uri) -> Result<Self> {
        match uri {
            Uri::Directory(path) => Self::try_from(path),
            Uri::File(path) => Self::try_from(path),
            _ => Err(eyre!("Expected directory got {uri:?}")),
        }
    }
}

impl TryFrom<PathBuf> for DirectoryExporter {
    type Error = color_eyre::eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let subpath = path.join("commercial");
        if !path.exists() {
            log::debug!("Creating directory: {}", &subpath.display());
            std::fs::create_dir_all(subpath)?;
        }
        match path.is_dir() {
            true => Ok(Self { path }),
            false => Err(eyre!(
                "Directory input must be a directory: {}",
                path.display()
            )),
        }
    }
}

impl std::fmt::Display for DirectoryExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}
