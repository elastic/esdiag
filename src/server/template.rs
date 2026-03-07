// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

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
    pub send_hosts: Vec<String>,
    pub exporter: String,
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
}

#[derive(Template)]
#[template(path = "hosts.html")]
pub struct HostsPage {
    pub auth_header: bool,
    pub debug: bool,
    pub desktop: bool,
    pub can_configure_output: bool,
    pub send_hosts: Vec<String>,
    pub exporter: String,
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
    pub hosts: Vec<HostsTableRow>,
    pub secrets: Vec<SecretTableRow>,
    pub secret_ids: Vec<String>,
    pub hosts_records_json: String,
    pub secrets_records_json: String,
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
