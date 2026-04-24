// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    data::{KnownHost, Uri},
    exporter::Exporter,
};
use askama::Template;
use serde::Serialize;
use std::collections::BTreeMap;

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
    pub show_advanced: bool,
    pub show_job_builder: bool,
    pub can_use_keystore: bool,
    pub output_secure: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
    pub show_keystore_bootstrap: bool,
}

#[derive(Template)]
#[template(path = "advanced.html")]
pub struct Advanced {
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub collect_hosts: Vec<String>,
    pub collect_secure_hosts_json: String,
    pub configured_local_path: String,
    pub configured_remote_target: String,
    pub default_save_dir: String,
    pub initial_send_mode: String,
    pub initial_local_target: String,
    pub initial_remote_target: String,
    pub kibana_url: String,
    pub key_id: Option<u64>,
    pub link_id: Option<u64>,
    pub process_options_json: String,
    pub send_secure_hosts_json: String,
    pub send_local_hosts: Vec<String>,
    pub send_remote_hosts: Vec<String>,
    pub upload_id: Option<u64>,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub show_advanced: bool,
    pub show_job_builder: bool,
    pub can_use_keystore: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
    pub show_keystore_bootstrap: bool,
}

#[derive(Template)]
#[template(path = "jobs.html")]
pub struct Jobs {
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub collect_hosts: Vec<String>,
    pub collect_secure_hosts_json: String,
    pub configured_local_path: String,
    pub configured_remote_target: String,
    pub default_save_dir: String,
    pub kibana_url: String,
    pub key_id: Option<u64>,
    pub link_id: Option<u64>,
    pub process_options_json: String,
    pub send_secure_hosts_json: String,
    pub send_local_hosts: Vec<String>,
    pub send_remote_hosts: Vec<String>,
    pub upload_id: Option<u64>,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub show_advanced: bool,
    pub show_job_builder: bool,
    pub can_use_keystore: bool,
    pub keystore_locked: bool,
    pub keystore_lock_time: i64,
    pub show_keystore_bootstrap: bool,
    // Saved job fields
    pub saved_job_name: Option<String>,
    pub saved_collect_mode: String,
    pub saved_collect_source: String,
    pub saved_known_host: String,
    pub saved_diagnostic_type: String,
    pub saved_collect_save: bool,
    pub saved_save_dir: String,
    pub saved_process_mode: String,
    pub saved_process_enabled: bool,
    pub saved_process_product: String,
    pub saved_process_diagnostic_type: String,
    pub saved_process_selected: String,
    pub saved_send_mode: String,
    pub saved_remote_target: String,
    pub saved_local_target: String,
    pub saved_local_directory: String,
    pub saved_user: String,
    pub saved_account: String,
    pub saved_case_number: String,
    pub saved_opportunity: String,
    pub saved_stale_host: bool,
    pub saved_message: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsModal {
    pub output_options: Vec<FooterOutputOption>,
    pub selected_output: String,
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
    pub kibana_url: String,
    pub stats: String,
    pub user: String,
    pub user_initial: char,
    pub version: String,
    pub theme_dark: bool,
    pub runtime_mode: String,
    pub show_advanced: bool,
    pub show_job_builder: bool,
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
#[template(path = "job/collection_processing.html")]
pub struct JobCollectionProcessing<'a> {
    pub job_id: u64,
    pub source: &'a str,
}

#[derive(Template)]
#[template(path = "job/collection_completed.html")]
pub struct JobCollectionCompleted<'a> {
    pub job_id: u64,
    pub source: &'a str,
    pub archive_path: &'a str,
}

#[derive(Template)]
#[template(path = "job/forward_completed.html")]
pub struct JobForwardCompleted<'a> {
    pub job_id: u64,
    pub source: &'a str,
    pub destination: &'a str,
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

#[derive(Template)]
#[template(path = "job/forward_processing.html")]
pub struct JobForwardProcessing<'a> {
    pub job_id: u64,
    pub source: &'a str,
}

#[derive(Clone, Serialize)]
pub struct FooterOutputOption {
    pub value: String,
    pub label: String,
}

pub fn build_footer_output_context(
    hosts_by_name: &BTreeMap<String, KnownHost>,
    send_hosts: &[String],
    exporter: &Exporter,
    preferred_target: Option<&str>,
) -> (Vec<FooterOutputOption>, String, String) {
    let exporter_value = exporter.target_uri();
    let selected_output = preferred_target
        .filter(|target| send_hosts.iter().any(|host| host == *target))
        .and_then(|target| preferred_target_matches_exporter(hosts_by_name, target, exporter).then_some(target))
        .map(str::to_string)
        .unwrap_or_else(|| exporter_value.clone());

    let mut output_options = send_hosts
        .iter()
        .map(|host| FooterOutputOption {
            value: host.clone(),
            label: host.clone(),
        })
        .collect::<Vec<_>>();

    if !output_options.iter().any(|option| option.value == selected_output) {
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

fn preferred_target_matches_exporter(
    hosts_by_name: &BTreeMap<String, KnownHost>,
    target: &str,
    exporter: &Exporter,
) -> bool {
    let Some(host) = hosts_by_name.get(target) else {
        return false;
    };
    host.get_url().to_string() == exporter.target_uri()
}

pub fn active_output_requires_keystore(
    hosts_by_name: &BTreeMap<String, KnownHost>,
    send_hosts: &[String],
    selected_output: &str,
    exporter: &Exporter,
) -> bool {
    if let Some(host) = hosts_by_name.get(selected_output) {
        return host.requires_keystore_secret();
    }

    send_hosts.iter().any(|host_name| {
        let Some(host) = hosts_by_name.get(host_name) else {
            return false;
        };
        let secure = host.requires_keystore_secret();
        if !secure {
            return false;
        }
        host.get_url().to_string() == exporter.target_uri()
    })
}

fn output_target_label(target: &str) -> String {
    match Uri::try_from(target.to_string()) {
        Ok(Uri::Directory(path)) => {
            let display = path.display().to_string();
            if display.ends_with('/') || display.ends_with('\\') {
                format!("dir: {display}")
            } else {
                format!("dir: {display}{}", std::path::MAIN_SEPARATOR)
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
    use super::{Jobs, active_output_requires_keystore, build_footer_output_context};
    use crate::{
        data::{HostRole, KnownHost, Product, Uri},
        exporter::Exporter,
    };
    use askama::Template;
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
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                None,
                false,
            ),
        );
        hosts.insert(
            "secure-prod".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("https://secure.example.com:9200").expect("url"),
                vec![HostRole::Send],
                None,
                false,
                Some("secure-prod".to_string()),
                None,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        tmp
    }

    #[test]
    fn footer_context_prefers_live_cli_output_over_saved_host_override() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_hosts();
        let send_hosts = vec!["localhost".to_string(), "secure-prod".to_string()];
        let exporter = Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/output"))).expect("directory exporter");

        let (options, selected_output, label) = build_footer_output_context(
            &KnownHost::parse_hosts_yml().unwrap_or_default(),
            &send_hosts,
            &exporter,
            Some("localhost"),
        );

        assert_eq!(selected_output, "file:///tmp/output/");
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

        let secure_exporter = Exporter::try_from(KnownHost::new_no_auth(
            Product::Elasticsearch,
            Url::parse("https://secure.example.com:9200").expect("secure url"),
            vec![HostRole::Send],
            None,
            false,
        ))
        .expect("secure exporter");
        let hosts_by_name = KnownHost::parse_hosts_yml().unwrap_or_default();
        assert!(active_output_requires_keystore(
            &hosts_by_name,
            &send_hosts,
            "secure-prod",
            &secure_exporter
        ));
        assert!(active_output_requires_keystore(
            &hosts_by_name,
            &send_hosts,
            &secure_exporter.target_uri(),
            &secure_exporter
        ));

        let dir_exporter =
            Exporter::try_from(Uri::Directory(PathBuf::from("/tmp/output"))).expect("directory exporter");
        assert!(!active_output_requires_keystore(
            &hosts_by_name,
            &send_hosts,
            &dir_exporter.target_uri(),
            &dir_exporter
        ));
    }

    #[test]
    fn jobs_template_does_not_seed_conflicting_workflow_root_signal() {
        let page = Jobs {
            auth_header: false,
            debug: false,
            desktop: false,
            collect_hosts: vec![],
            collect_secure_hosts_json: "[]".to_string(),
            configured_local_path: String::new(),
            configured_remote_target: String::new(),
            default_save_dir: "/tmp".to_string(),
            kibana_url: String::new(),
            key_id: None,
            link_id: None,
            process_options_json: "{}".to_string(),
            send_secure_hosts_json: "[]".to_string(),
            send_local_hosts: vec![],
            send_remote_hosts: vec![],
            upload_id: None,
            stats: "{}".to_string(),
            user: "tester@example.com".to_string(),
            user_initial: 'T',
            version: "test".to_string(),
            theme_dark: false,
            runtime_mode: "user".to_string(),
            show_advanced: true,
            show_job_builder: true,
            can_use_keystore: true,
            keystore_locked: false,
            keystore_lock_time: 0,
            show_keystore_bootstrap: false,
            saved_job_name: None,
            saved_collect_mode: "upload".to_string(),
            saved_collect_source: "upload-file".to_string(),
            saved_known_host: String::new(),
            saved_diagnostic_type: "standard".to_string(),
            saved_collect_save: false,
            saved_save_dir: "/tmp".to_string(),
            saved_process_mode: "process".to_string(),
            saved_process_enabled: true,
            saved_process_product: "elasticsearch".to_string(),
            saved_process_diagnostic_type: "standard".to_string(),
            saved_process_selected: String::new(),
            saved_send_mode: "local".to_string(),
            saved_remote_target: String::new(),
            saved_local_target: "directory".to_string(),
            saved_local_directory: "/tmp".to_string(),
            saved_user: String::new(),
            saved_account: String::new(),
            saved_case_number: String::new(),
            saved_opportunity: String::new(),
            saved_stale_host: false,
            saved_message: String::new(),
        };

        let html = page.render().expect("jobs template renders");
        assert!(
            !html.contains(r#"data-signals:workflow="{}""#),
            "jobs page should not seed a top-level workflow object that overrides nested workflow signals"
        );
    }
}
