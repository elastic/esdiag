use eyre::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub active_target: Option<String>,
    pub kibana_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacySettingsMigration {
    NoLegacySettings,
    AlreadyConfigured,
    Migrated,
    NotRepresentable,
}

impl Settings {
    pub fn path() -> Result<PathBuf> {
        let hosts_path = super::KnownHost::get_hosts_path();
        let esdiag_dir = hosts_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
        if !esdiag_dir.exists() {
            fs::create_dir_all(&esdiag_dir)?;
        }
        Ok(esdiag_dir.join("settings.yml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let settings: Settings = yaml_serde::from_str(&content)?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let content = yaml_serde::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Migrates a legacy active target only when it already describes one
    /// linked Elasticsearch/Kibana deployment. The legacy file remains intact
    /// and is backed up before shared configuration is written.
    pub fn migrate_to_application_config() -> Result<LegacySettingsMigration> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(LegacySettingsMigration::NoLegacySettings);
        }
        let legacy = Self::load()?;
        let mut config = super::ApplicationConfig::load()?;
        if config.output.default.is_some() {
            return Ok(LegacySettingsMigration::AlreadyConfigured);
        }
        let Some(active_target) = legacy.active_target else {
            return Ok(LegacySettingsMigration::NotRepresentable);
        };
        let Some(output) = super::KnownHost::get_known(&active_target) else {
            return Ok(LegacySettingsMigration::NotRepresentable);
        };
        if output.app() != Some(super::Application::Elasticsearch) || !output.has_role(super::HostRole::Send) {
            return Ok(LegacySettingsMigration::NotRepresentable);
        }
        let Some(viewer_name) = output.viewer() else {
            return Ok(LegacySettingsMigration::NotRepresentable);
        };
        let Some(viewer) = super::KnownHost::get_known(&viewer_name.to_string()) else {
            return Ok(LegacySettingsMigration::NotRepresentable);
        };
        if viewer.app() != Some(super::Application::Kibana) || !viewer.has_role(super::HostRole::View) {
            return Ok(LegacySettingsMigration::NotRepresentable);
        }
        if let Some(legacy_kibana) = legacy.kibana_url
            && viewer.concrete_url().map(|url| url.to_string()) != Some(legacy_kibana)
        {
            return Ok(LegacySettingsMigration::NotRepresentable);
        }

        let backup = path.with_extension("yml.backup");
        if !backup.exists() {
            fs::copy(&path, &backup)?;
        }
        config.output.default = Some(active_target);
        config.save()?;
        Ok(LegacySettingsMigration::Migrated)
    }
}

#[cfg(test)]
mod tests {
    use super::{LegacySettingsMigration, Settings};
    use crate::data::{Application, ApplicationConfig, HostRole, KnownHost, KnownHostBuilder};
    use std::collections::BTreeMap;
    use url::Url;

    #[test]
    fn migrates_linked_legacy_output_after_creating_a_backup() {
        let env = crate::TestEnv::new();
        let output = KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("output url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .viewer(Some("output-kibana".to_string()))
            .build()
            .expect("output host");
        let viewer = KnownHostBuilder::new(Url::parse("https://kb.example:5601").expect("viewer url"))
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .expect("viewer host");
        KnownHost::write_hosts_yml(&BTreeMap::from([
            ("output".to_string(), output),
            ("output-kibana".to_string(), viewer),
        ]))
        .expect("write hosts");
        Settings {
            active_target: Some("output".to_string()),
            kibana_url: Some("https://kb.example:5601/".to_string()),
        }
        .save()
        .expect("write legacy settings");

        let result = Settings::migrate_to_application_config().expect("migrate settings");

        assert_eq!(result, LegacySettingsMigration::Migrated);
        assert_eq!(
            ApplicationConfig::load()
                .expect("load shared config")
                .output
                .default
                .as_deref(),
            Some("output")
        );
        assert!(env.settings_path.with_extension("yml.backup").exists());
        assert!(env.settings_path.exists());
    }
}
