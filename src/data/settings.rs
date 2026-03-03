use eyre::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub active_target: Option<String>,
    pub kibana_url: Option<String>,
}

impl Settings {
    fn get_path() -> Result<PathBuf> {
        let hosts_path = super::KnownHost::get_hosts_path();
        let esdiag_dir = hosts_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        if !esdiag_dir.exists() {
            fs::create_dir_all(&esdiag_dir)?;
        }
        Ok(esdiag_dir.join("settings.yml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::get_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let settings: Settings = serde_yaml::from_str(&content)?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::get_path()?;
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}
