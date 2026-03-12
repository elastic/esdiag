// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    data::{KnownHost, Uri},
    exporter::Exporter,
};
use askama::Template;
use serde::Serialize;

#[derive(Template)]
#[template(path = "error.html")]
pub struct Error<'e> {
    pub id: &'e str,
    pub error: &'e str,
    pub message: &'e str,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub can_configure_output: bool,
    pub output_options: Vec<FooterOutputOption>,
    pub selected_output: String,
    pub exporter_label: String,
    pub active_output_secure: bool,
    pub kibana_url: String,
    pub key_id: Option<u64>,
    pub link_id: Option<u64>,
    pub upload_id: Option<u64>,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub can_use_keystore: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
    pub show_keystore_bootstrap: bool,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsModal {
    pub hosts: Vec<String>,
    pub active_target: String,
    pub kibana_url: String,
    pub mode: String,
    pub can_update_exporter: bool,
}

#[derive(Template)]
#[template(path = "keystore/unlock.html")]
pub struct KeystoreUnlockModal {}

#[derive(Template)]
#[template(path = "keystore/process_unlock.html")]
pub struct KeystoreProcessUnlockModal {}

#[derive(Template)]
#[template(path = "keystore/bootstrap.html")]
pub struct KeystoreBootstrapModal {
    pub migrate: bool,
}

#[derive(Template)]
#[template(path = "keystore/hosts_manager.html")]
pub struct HostsManagerModal {
    pub hosts: Vec<String>,
    pub secret_names: Vec<String>,
    pub keystore_locked: bool,
}

#[derive(Template)]
#[template(path = "hosts/host_panel.html")]
pub struct HostsTablePanelTemplate {
    pub rows_html: String,
    pub keystore_locked: bool,
}

#[derive(Template)]
#[template(path = "hosts/secret_panel.html")]
pub struct SecretsTablePanelTemplate {
    pub rows_html: String,
    pub keystore_locked: bool,
}

#[derive(Template)]
#[template(path = "hosts/cluster_panel.html")]
pub struct DiagnosticClustersTablePanelTemplate {
    pub rows_html: String,
    pub keystore_locked: bool,
}

#[derive(Template)]
#[template(path = "hosts/host_row.html")]
pub struct HostsTableRowTemplate {
    pub row_id: usize,
    pub host: HostsTableRow,
    pub secret_ids: Vec<String>,
    pub keystore_locked: bool,
    pub editing: bool,
    pub persisted: bool,
}

#[derive(Template)]
#[template(path = "hosts/secret_row.html")]
pub struct SecretsTableRowTemplate {
    pub row_id: usize,
    pub secret: SecretTableRow,
    pub editing: bool,
    pub persisted: bool,
    pub keystore_locked: bool,
}

#[derive(Template)]
#[template(path = "hosts/cluster_row.html")]
pub struct DiagnosticClusterTableRowTemplate {
    pub row_id: usize,
    pub cluster: DiagnosticClusterTableRow,
    pub secret_ids: Vec<String>,
    pub keystore_locked: bool,
    pub editing: bool,
    pub persisted: bool,
}

#[derive(Clone, Serialize)]
pub struct HostsTableRow {
    pub name: String,
    pub auth: String,
    pub app: String,
    pub url: String,
    pub roles: String,
    pub viewer: String,
    pub accept_invalid_certs: bool,
    pub cloud_id: String,
    pub secret: String,
    pub apikey: String,
    pub username: String,
    pub password: String,
}

#[derive(Clone, Serialize)]
pub struct SecretTableRow {
    pub secret_id: String,
    pub auth_type: String,
    pub username: String,
}

#[derive(Clone, Serialize)]
pub struct DiagnosticClusterTableRow {
    pub name: String,
    pub auth: String,
    pub secret: String,
    pub accept_invalid_certs: bool,
    pub elasticsearch_url: String,
    pub kibana_url: String,
}

#[derive(Template)]
#[template(path = "hosts.html")]
pub struct HostsPage {
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub can_configure_output: bool,
    pub output_options: Vec<FooterOutputOption>,
    pub selected_output: String,
    pub exporter_label: String,
    pub active_output_secure: bool,
    pub kibana_url: String,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub can_use_keystore: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
    pub show_keystore_bootstrap: bool,
    pub hosts_panel_html: String,
    pub secrets_panel_html: String,
    pub clusters_panel_html: String,
}
#[derive(Template)]
#[template(path = "job/completed.html")]
pub struct JobCompleted<'a> {
    pub job_id: u64,
    pub diagnostic_id: &'a str,
    pub docs_created: &'a u32,
    pub duration: &'a str,
    pub source: &'a str,
    pub kibana_link: &'a str,
    pub product: &'a str,
}

#[derive(Template)]
#[template(path = "job/failed.html")]
pub struct JobFailed<'a> {
    pub job_id: u64,
    pub error: &'a str,
    pub source: &'a str,
}

#[derive(Template)]
#[template(path = "job/processing.html")]
pub struct JobProcessing<'a> {
    pub job_id: u64,
    pub source: &'a str,
}

#[derive(Clone, Serialize)]
pub struct FooterOutputOption {
    pub value: String,
    pub label: String,
}

pub fn build_footer_output_context(
    send_hosts: &[String],
    exporter: &Exporter,
    preferred_target: Option<&str>,
) -> (Vec<FooterOutputOption>, String, String) {
    let exporter_value = exporter.target_value();
    let selected_output = preferred_target
        .filter(|target| send_hosts.iter().any(|host| host == *target))
        .and_then(|target| preferred_target_matches_exporter(target, exporter).then_some(target))
        .map(str::to_string)
        .unwrap_or_else(|| exporter_value.clone());

    let mut output_options = send_hosts
        .iter()
        .map(|host| FooterOutputOption {
            value: host.clone(),
            label: host.clone(),
        })
        .collect::<Vec<_>>();

    if !output_options
        .iter()
        .any(|option| option.value == selected_output)
    {
        let label = if selected_output == exporter_value {
            exporter.target_label()
        } else {
            output_target_label(&selected_output)
        };
        output_options.insert(
            0,
            FooterOutputOption {
                value: selected_output.clone(),
                label,
            },
        );
    }

    let exporter_label = if selected_output == exporter_value {
        exporter.target_label()
    } else {
        output_target_label(&selected_output)
    };

    (output_options, selected_output, exporter_label)
}

fn preferred_target_matches_exporter(target: &str, exporter: &Exporter) -> bool {
    let Some(host) = KnownHost::get_known(&target.to_string()) else {
        return false;
    };
    host.get_url().to_string() == exporter.target_value()
}

pub fn active_output_requires_keystore(
    send_hosts: &[String],
    selected_output: &str,
    exporter: &Exporter,
) -> bool {
    if let Some(host) = KnownHost::get_known(&selected_output.to_string()) {
        return !matches!(host, KnownHost::NoAuth { .. });
    }

    send_hosts.iter().any(|host_name| {
        let Some(host) = KnownHost::get_known(host_name) else {
            return false;
        };
        let secure = !matches!(host, KnownHost::NoAuth { .. });
        if !secure {
            return false;
        }
        host.get_url().to_string() == exporter.target_value()
    })
}

fn output_target_label(target: &str) -> String {
    match Uri::try_from(target.to_string()) {
        Ok(Uri::Directory(path)) => {
            let display = path.display().to_string();
            if display.ends_with('/') {
                format!("dir: {display}")
            } else {
                format!("dir: {display}/")
            }
        }
        Ok(Uri::File(path)) => format!("file: {}", path.display()),
        Ok(Uri::Stream) => "stdout: -".to_string(),
        Ok(Uri::KnownHost(_))
        | Ok(Uri::ElasticCloud(_))
        | Ok(Uri::ElasticCloudAdmin(_))
        | Ok(Uri::ElasticGovCloudAdmin(_)) => target.to_string(),
        Ok(Uri::ServiceLink(url)) | Ok(Uri::ServiceLinkNoAuth(url)) | Ok(Uri::Url(url)) => {
            format!("url: {url}")
        }
        Err(_) => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{active_output_requires_keystore, build_footer_output_context};
    use crate::{
        data::{HostRole, KnownHost, Product, Uri},
        exporter::Exporter,
    };
    use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_hosts() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let hosts_path = config_dir.join("hosts.yml");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_HOSTS", &hosts_path);
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "localhost".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "secure-prod".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: None,
                app: Product::Elasticsearch,
                cloud_id: None,
                roles: vec![HostRole::Send],
                secret: Some("secure-prod".to_string()),
                viewer: None,
                url: Url::parse("https://secure.example.com:9200").expect("url"),
            },
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        tmp
    }

    #[test]
    fn footer_context_prefers_live_cli_output_over_saved_host_override() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_hosts();
        let send_hosts = vec!["localhost".to_string(), "secure-prod".to_string()];
        let exporter = Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/output")))
            .expect("directory exporter");

        let (options, selected_output, label) =
            build_footer_output_context(&send_hosts, &exporter, Some("localhost"));

        assert_eq!(selected_output, "/tmp/output");
        assert_eq!(label, "dir: /tmp/output/");
        assert_eq!(
            options.first().map(|option| option.label.as_str()),
            Some("dir: /tmp/output/")
        );
    }

    #[test]
    fn active_output_security_tracks_selected_host_or_matching_exporter() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_hosts();
        let send_hosts = vec!["localhost".to_string(), "secure-prod".to_string()];

        let secure_exporter = Exporter::try_from(KnownHost::NoAuth {
            app: Product::Elasticsearch,
            roles: vec![HostRole::Send],
            viewer: None,
            url: Url::parse("https://secure.example.com:9200").expect("secure url"),
        })
        .expect("secure exporter");
        assert!(active_output_requires_keystore(
            &send_hosts,
            "secure-prod",
            &secure_exporter
        ));
        assert!(active_output_requires_keystore(
            &send_hosts,
            &secure_exporter.target_value(),
            &secure_exporter
        ));

        let dir_exporter = Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/output")))
            .expect("directory exporter");
        assert!(!active_output_requires_keystore(
            &send_hosts,
            &dir_exporter.target_value(),
            &dir_exporter
        ));
    }
}
