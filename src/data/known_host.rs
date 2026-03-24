// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::keystore::upsert_secret_auth_batch;
use crate::data::{
    Auth, Product, SecretAuth, get_password_from_env, resolve_secret_auth as resolve_secret_by_id,
};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::{
    collections::BTreeMap,
    env,
    fmt::{Display, Formatter},
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    str::FromStr,
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostRole {
    Collect,
    Send,
    View,
}

impl std::fmt::Display for HostRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Collect => write!(f, "collect"),
            Self::Send => write!(f, "send"),
            Self::View => write!(f, "view"),
        }
    }
}

impl FromStr for HostRole {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "collect" => Ok(Self::Collect),
            "send" => Ok(Self::Send),
            "view" => Ok(Self::View),
            _ => Err(eyre!("Unknown host role '{s}'")),
        }
    }
}

fn default_collect_roles() -> Vec<HostRole> {
    vec![HostRole::Collect]
}

fn roles_is_default_collect(roles: &[HostRole]) -> bool {
    roles.len() == 1 && roles[0] == HostRole::Collect
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ElasticCloud {
    ElasticGovCloudAdmin,
    ElasticCloudAdmin,
    ElasticCloud,
}

impl TryFrom<&Url> for ElasticCloud {
    type Error = String;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if url.domain() == Some("admin.us-gov-east-1.aws.elastic-cloud.com") {
            Ok(ElasticCloud::ElasticGovCloudAdmin)
        } else if url.domain() == Some("admin.found.no") {
            Ok(ElasticCloud::ElasticCloudAdmin)
        } else if url.domain() == Some("cloud.elastic.co") {
            Ok(ElasticCloud::ElasticCloud)
        } else {
            Err(String::from("Not an elastic Cloud URL"))
        }
    }
}

impl Display for ElasticCloud {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ElasticCloud::ElasticGovCloudAdmin => write!(f, "ElasticGovCloudAdmin"),
            ElasticCloud::ElasticCloudAdmin => write!(f, "ElasticCloudAdmin"),
            ElasticCloud::ElasticCloud => write!(f, "ElasticCloud"),
        }
    }
}

pub struct KnownHostBuilder {
    accept_invalid_certs: bool,
    apikey: Option<String>,
    product: Product,
    cloud_id: Option<ElasticCloud>,
    password: Option<String>,
    roles: Vec<HostRole>,
    secret: Option<String>,
    url: Url,
    username: Option<String>,
    viewer: Option<String>,
}

impl KnownHostBuilder {
    pub fn new(url: Url) -> Self {
        KnownHostBuilder {
            accept_invalid_certs: false,
            apikey: None,
            product: Product::Elasticsearch,
            cloud_id: None,
            password: None,
            roles: default_collect_roles(),
            secret: None,
            url,
            username: None,
            viewer: None,
        }
    }

    pub fn accept_invalid_certs(self, accept_invalid_certs: bool) -> Self {
        Self {
            accept_invalid_certs,
            ..self
        }
    }

    pub fn apikey(self, apikey: Option<String>) -> Self {
        Self { apikey, ..self }
    }

    pub fn password(self, password: Option<String>) -> Self {
        Self { password, ..self }
    }

    pub fn product(self, product: Product) -> Self {
        Self { product, ..self }
    }

    pub fn secret(self, secret: Option<String>) -> Self {
        Self { secret, ..self }
    }

    pub fn roles(self, roles: Vec<HostRole>) -> Self {
        Self { roles, ..self }
    }

    pub fn username(self, username: Option<String>) -> Self {
        Self { username, ..self }
    }

    pub fn viewer(self, viewer: Option<String>) -> Self {
        Self { viewer, ..self }
    }

    fn update_cloud_api_path(&mut self) {
        let mut url = self.url.clone();
        self.cloud_id = ElasticCloud::try_from(&url).ok();
        if self.cloud_id.is_none() {
            return;
        }

        // Desired URL format is https://{domain}/api/v1/deployments/{deployment_id}/elasticsearch/elasticsearch/proxy/
        let deployment_id = url.clone();
        let deployment_id = deployment_id
            .path()
            .split('/')
            .skip_while(|segment| *segment != "deployments")
            .nth(1)
            .unwrap_or("");
        let new_segments: Vec<&str> = match self.product {
            Product::Elasticsearch => {
                let product = match url.domain() {
                    Some("admin.found.no") => "main-elasticsearch",
                    _ => "elasticsearch",
                };
                vec![
                    "api",
                    "v1",
                    "deployments",
                    deployment_id,
                    "elasticsearch",
                    product,
                    "proxy",
                ]
            }
            _ => Vec::new(),
        };
        // Only modify the path if we have new segments
        if !new_segments.is_empty() {
            let mut path_segments = url
                .path_segments_mut()
                .expect("Failed to get path segments");
            path_segments.clear().extend(new_segments);
        }

        tracing::debug!("Updated Cloud API URL: {}", url);
        self.url = url;
    }

    pub fn build(mut self) -> Result<KnownHost> {
        self.update_cloud_api_path();
        match (self.apikey, self.username, self.password, self.secret) {
            (Some(apikey), None, None, secret) => Ok(KnownHost::ApiKey {
                accept_invalid_certs: self.accept_invalid_certs,
                apikey: Some(apikey),
                app: self.product,
                cloud_id: self.cloud_id,
                roles: self.roles,
                secret,
                url: self.url,
                viewer: self.viewer,
            }),
            (None, Some(username), Some(password), secret) => Ok(KnownHost::Basic {
                accept_invalid_certs: self.accept_invalid_certs,
                app: self.product,
                password: Some(password),
                roles: self.roles,
                secret,
                url: self.url,
                username: Some(username),
                viewer: self.viewer,
            }),
            (None, None, None, None) => Ok(KnownHost::NoAuth {
                app: self.product,
                roles: self.roles,
                url: self.url,
                viewer: self.viewer,
            }),
            // Allow hosts to persist only a secret reference.
            (None, None, None, Some(secret)) => Ok(KnownHost::Basic {
                accept_invalid_certs: self.accept_invalid_certs,
                app: self.product,
                password: None,
                roles: self.roles,
                secret: Some(secret),
                url: self.url,
                username: None,
                viewer: self.viewer,
            }),
            _ => Err(eyre!("Invalid KnownHost configuration")),
        }
    }

    pub fn build_with_secret_auth(mut self, secret_auth: SecretAuth) -> Result<KnownHost> {
        self.update_cloud_api_path();

        if self.apikey.is_some() || self.username.is_some() || self.password.is_some() {
            return Err(eyre!(
                "Invalid KnownHost configuration: explicit credentials conflict with secret auth"
            ));
        }

        let secret = self
            .secret
            .take()
            .ok_or_else(|| eyre!("Invalid KnownHost configuration: missing secret reference"))?;

        match secret_auth {
            SecretAuth::ApiKey { .. } => Ok(KnownHost::ApiKey {
                accept_invalid_certs: self.accept_invalid_certs,
                apikey: None,
                app: self.product,
                cloud_id: self.cloud_id,
                roles: self.roles,
                secret: Some(secret),
                url: self.url,
                viewer: self.viewer,
            }),
            SecretAuth::Basic { .. } => Ok(KnownHost::Basic {
                accept_invalid_certs: self.accept_invalid_certs,
                app: self.product,
                password: None,
                roles: self.roles,
                secret: Some(secret),
                url: self.url,
                username: None,
                viewer: self.viewer,
            }),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "auth")]
pub enum KnownHost {
    /// A host using API key authentication
    ApiKey {
        accept_invalid_certs: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        apikey: Option<String>,
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<ElasticCloud>,
        #[serde(
            default = "default_collect_roles",
            skip_serializing_if = "roles_is_default_collect"
        )]
        roles: Vec<HostRole>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        url: Url,
    },
    /// A host using basic username/password authentication
    Basic {
        accept_invalid_certs: bool,
        app: Product,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(
            default = "default_collect_roles",
            skip_serializing_if = "roles_is_default_collect"
        )]
        roles: Vec<HostRole>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        url: Url,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    /// A host with no authentication
    #[serde(alias = "None")]
    NoAuth {
        app: Product,
        #[serde(
            default = "default_collect_roles",
            skip_serializing_if = "roles_is_default_collect"
        )]
        roles: Vec<HostRole>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: Option<String>,
        url: Url,
    },
}

impl KnownHost {
    pub fn app(&self) -> &Product {
        match self {
            Self::ApiKey { app, .. } => app,
            Self::Basic { app, .. } => app,
            Self::NoAuth { app, .. } => app,
        }
    }

    pub fn get_url(&self) -> Url {
        match self {
            Self::ApiKey { url, .. } => url.clone(),
            Self::Basic { url, .. } => url.clone(),
            Self::NoAuth { url, .. } => url.clone(),
        }
    }

    pub fn roles(&self) -> &[HostRole] {
        match self {
            Self::ApiKey { roles, .. } => roles,
            Self::Basic { roles, .. } => roles,
            Self::NoAuth { roles, .. } => roles,
        }
    }

    pub fn has_role(&self, role: HostRole) -> bool {
        self.roles().contains(&role)
    }

    pub fn viewer(&self) -> Option<&str> {
        match self {
            Self::ApiKey { viewer, .. } => viewer.as_deref(),
            Self::Basic { viewer, .. } => viewer.as_deref(),
            Self::NoAuth { viewer, .. } => viewer.as_deref(),
        }
    }

    pub fn accept_invalid_certs(&self) -> bool {
        match self {
            Self::ApiKey {
                accept_invalid_certs,
                ..
            } => *accept_invalid_certs,
            Self::Basic {
                accept_invalid_certs,
                ..
            } => *accept_invalid_certs,
            Self::NoAuth { .. } => false,
        }
    }

    pub fn requires_keystore_secret(&self) -> bool {
        matches!(
            self,
            Self::ApiKey {
                secret: Some(_),
                ..
            } | Self::Basic {
                secret: Some(_),
                ..
            }
        )
    }

    pub fn get_auth(&self) -> Result<Auth> {
        match self {
            Self::ApiKey { apikey, secret, .. } => {
                resolve_auth_with_precedence(secret, apikey.clone(), None)
            }
            Self::Basic {
                username,
                password,
                secret,
                ..
            } => resolve_auth_with_precedence(secret, None, username.clone().zip(password.clone())),
            Self::NoAuth { .. } => Ok(Auth::None),
        }
    }

    pub fn save(mut self, name: &str) -> Result<String> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let mut hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                tracing::error!("Error parsing hosts.yml: {}", e);
                return Err(eyre!("Error parsing hosts.yml"));
            }
        };
        self.normalize_and_validate_roles(name)?;
        self.strip_plaintext_when_secret_present();
        match self {
            Self::ApiKey { .. } => {
                hosts.insert(name.to_owned(), self);
            }
            Self::Basic { .. } => {
                hosts.insert(name.to_owned(), self);
            }
            Self::NoAuth { .. } => {
                hosts.insert(name.to_owned(), self);
            }
        }
        KnownHost::write_hosts_yml(&hosts)
    }

    fn strip_plaintext_when_secret_present(&mut self) {
        match self {
            Self::ApiKey { apikey, secret, .. } => {
                if secret.is_some() {
                    *apikey = None;
                }
            }
            Self::Basic {
                username,
                password,
                secret,
                ..
            } => {
                if secret.is_some() {
                    *username = None;
                    *password = None;
                }
            }
            Self::NoAuth { .. } => {}
        }
    }

    pub fn get_known(host: &String) -> Option<Self> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                tracing::error!("Error parsing hosts.yml: {}", e);
                return None;
            }
        };
        tracing::debug!(
            "Known hosts: {}",
            hosts
                .clone()
                .into_keys()
                .collect::<Vec<String>>()
                .join(", ")
        );
        hosts.get(host).cloned()
    }

    pub fn has_legacy_secret(&self) -> bool {
        match self {
            Self::ApiKey { apikey, .. } => apikey.is_some(),
            Self::Basic {
                username, password, ..
            } => username.is_some() || password.is_some(),
            Self::NoAuth { .. } => false,
        }
    }

    pub fn set_secret_reference(&mut self, secret_id: String) {
        match self {
            Self::ApiKey { apikey, secret, .. } => {
                *apikey = None;
                *secret = Some(secret_id);
            }
            Self::Basic {
                username,
                password,
                secret,
                ..
            } => {
                *username = None;
                *password = None;
                *secret = Some(secret_id);
            }
            Self::NoAuth { .. } => {}
        }
    }

    pub fn migrate_hosts_to_keystore(keystore_password: &str) -> Result<(usize, usize)> {
        let mut hosts = Self::parse_hosts_yml()?;
        let mut migrated = 0_usize;
        let mut unchanged = 0_usize;
        let total = hosts.len();
        let mut pending = Vec::new();

        tracing::info!("Starting keystore migration for {total} host(s).");

        for (index, (name, host)) in hosts.iter_mut().enumerate() {
            match host.legacy_auth() {
                Some(auth) => {
                    tracing::debug!(
                        "Preparing host {}/{} for keystore migration: {}",
                        index + 1,
                        total,
                        name
                    );
                    pending.push((name.clone(), auth));
                    host.set_secret_reference(name.clone());
                    migrated += 1;
                }
                None => {
                    tracing::debug!(
                        "Skipping host {}/{} without legacy credentials: {}",
                        index + 1,
                        total,
                        name
                    );
                    unchanged += 1;
                }
            }
        }

        if !pending.is_empty() {
            tracing::info!(
                "Writing {} migrated secret(s) to keystore in a single batch.",
                pending.len()
            );
            upsert_secret_auth_batch(pending, keystore_password)?;
        }

        tracing::info!("Writing migrated host references back to hosts.yml.");
        Self::write_hosts_yml(&hosts)?;
        Ok((migrated, unchanged))
    }

    pub fn list_by_role(role: HostRole) -> Result<Vec<String>> {
        let hosts = Self::parse_hosts_yml()?;
        let mut names: Vec<String> = hosts
            .iter()
            .filter_map(|(name, host)| host.has_role(role.clone()).then_some(name.clone()))
            .collect();
        names.sort();
        Ok(names)
    }

    fn legacy_auth(&self) -> Option<SecretAuth> {
        match self {
            Self::ApiKey {
                apikey: Some(apikey),
                ..
            } => Some(SecretAuth::ApiKey {
                apikey: apikey.clone(),
            }),
            Self::Basic {
                username: Some(username),
                password: Some(password),
                ..
            } => Some(SecretAuth::Basic {
                username: username.clone(),
                password: password.clone(),
            }),
            _ => None,
        }
    }

    pub fn list_all() -> Option<Vec<String>> {
        let hosts = KnownHost::parse_hosts_yml().ok()?;
        let mut names: Vec<String> = hosts.keys().cloned().collect();
        names.sort();
        Some(names)
    }

    pub fn from_url(url: &Url) -> Self {
        KnownHost::NoAuth {
            app: Product::Elasticsearch,
            roles: default_collect_roles(),
            viewer: None,
            url: url.clone(),
        }
    }

    pub fn get_hosts_path() -> PathBuf {
        match env::var("ESDIAG_HOSTS") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                let home_dir = match std::env::consts::OS {
                    "windows" => std::env::var("USERPROFILE").expect("Failed to get USERPROFILE"),
                    "linux" | "macos" => std::env::var("HOME").expect("Failed to get HOME"),
                    os => panic!("Unknown home directory for operating system: {os} "),
                };
                let home_dir = PathBuf::from(home_dir);
                // Check if the `.esdiag` directory exists, if not, create it
                let esdiag = home_dir.join(".esdiag");
                if !esdiag.exists() {
                    std::fs::create_dir(&esdiag).expect("Failed to create ~/.esdiag directory");
                }

                home_dir.join(".esdiag").join("hosts.yml")
            }
        }
    }

    /// Loads hosts from the ~/.esdiag/hosts.yml (defalt) file
    pub fn parse_hosts_yml() -> Result<BTreeMap<String, KnownHost>> {
        let path = KnownHost::get_hosts_path();
        tracing::debug!("Parsing {:?}", path);
        match path.is_file() {
            true => {
                let file = File::open(path)?;
                let reader = BufReader::new(file);
                let mut hosts: BTreeMap<String, KnownHost> = serde_yaml::from_reader(reader)?;
                for (name, host) in hosts.iter_mut() {
                    host.normalize_and_validate_roles(name)?;
                }
                validate_viewer_links(&hosts)?;
                Ok(hosts)
            }
            false => {
                tracing::info!("No hosts, file creating {:?}", path);
                File::create(path)?;
                Ok(BTreeMap::new())
            }
        }
    }

    pub fn write_hosts_yml(hosts: &BTreeMap<String, KnownHost>) -> Result<String> {
        let path = KnownHost::get_hosts_path();
        let mut hosts = hosts.clone();
        for (name, host) in hosts.iter_mut() {
            host.normalize_and_validate_roles(name)?;
        }
        validate_viewer_links(&hosts)?;
        tracing::debug!(
            "Writing hosts: {} to {:?}",
            hosts
                .clone()
                .into_keys()
                .collect::<Vec<String>>()
                .join(", "),
            path
        );
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        serde_yaml::to_writer(writer, &hosts)?;
        Ok(format!("{}", &path.display()))
    }

    fn normalize_and_validate_roles(&mut self, host_name: &str) -> Result<()> {
        let app = self.app().clone();
        let roles = match self {
            Self::ApiKey { roles, .. } => roles,
            Self::Basic { roles, .. } => roles,
            Self::NoAuth { roles, .. } => roles,
        };

        if roles.is_empty() {
            roles.push(HostRole::Collect);
        }
        roles.sort_by_key(|r| match r {
            HostRole::Collect => 0,
            HostRole::Send => 1,
            HostRole::View => 2,
        });
        roles.dedup();

        for role in roles.iter() {
            match role {
                HostRole::Collect => {}
                HostRole::Send if app == Product::Elasticsearch => {}
                HostRole::View if app == Product::Kibana => {}
                HostRole::Send => {
                    return Err(eyre!(
                        "Host '{host_name}' role 'send' is only valid for Elasticsearch hosts"
                    ));
                }
                HostRole::View => {
                    return Err(eyre!(
                        "Host '{host_name}' role 'view' is only valid for Kibana hosts"
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Display for KnownHost {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey {
                app, cloud_id, url, ..
            } => {
                let cloud_id = match cloud_id {
                    Some(id) => id.to_string(),
                    None => "None".to_string(),
                };
                write!(fmt, "KnownHost ApiKey: {} {} {}", app, url, cloud_id,)
            }
            Self::Basic {
                app, url, username, ..
            } => {
                let username = username
                    .clone()
                    .unwrap_or_else(|| "<secret-auth>".to_string());
                write!(fmt, "KnownHost Basic: {} {}@ {}", app, username, url,)
            }
            Self::NoAuth { app, url, .. } => write!(fmt, "KnownHost NoAuth: {} {}", app, url),
        }
    }
}

fn validate_viewer_links(hosts: &BTreeMap<String, KnownHost>) -> Result<()> {
    for (name, host) in hosts {
        if let Some(viewer_name) = host.viewer() {
            if !host.has_role(HostRole::Send) {
                return Err(eyre!(
                    "Host '{name}' sets 'viewer' but does not have required role 'send'"
                ));
            }
            let Some(viewer_host) = hosts.get(viewer_name) else {
                return Err(eyre!(
                    "Host '{name}' references unknown viewer host '{viewer_name}'"
                ));
            };
            if !viewer_host.has_role(HostRole::View) {
                return Err(eyre!(
                    "Host '{name}' viewer '{viewer_name}' must include role 'view'"
                ));
            }
        }
    }
    Ok(())
}

fn resolve_auth_with_precedence(
    secret_id: &Option<String>,
    legacy_apikey: Option<String>,
    legacy_basic: Option<(String, String)>,
) -> Result<Auth> {
    if let Some(secret_id) = secret_id {
        let auth = resolve_explicit_secret(secret_id)?;
        if legacy_apikey.is_some() || legacy_basic.is_some() {
            tracing::warn!(
                "Legacy credentials exist in hosts.yml and keystore entry '{secret_id}' exists; using keystore credentials."
            );
        }
        return Ok(auth);
    }

    if let Some(apikey) = legacy_apikey {
        return Ok(Auth::Apikey(apikey));
    }
    if let Some((username, password)) = legacy_basic {
        return Ok(Auth::Basic(username, password));
    }
    Ok(Auth::None)
}

fn resolve_explicit_secret(secret_id: &str) -> Result<Auth> {
    let keystore_password = get_password_from_env()
        .map_err(|err| eyre!("Host references secret '{secret_id}' but {err}"))?;
    let secret = resolve_secret_by_id(secret_id, &keystore_password)?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    secret_auth_to_auth(secret)
}

fn secret_auth_to_auth(secret: SecretAuth) -> Result<Auth> {
    match secret {
        SecretAuth::ApiKey { apikey } => Ok(Auth::Apikey(apikey)),
        SecretAuth::Basic { username, password } => Ok(Auth::Basic(username, password)),
    }
}

impl FromStr for KnownHost {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match KnownHost::get_known(&s.to_string()) {
            Some(host) => Ok(host),
            None => Err(()),
        }
    }
}

impl From<KnownHost> for Url {
    fn from(host: KnownHost) -> Url {
        match host {
            KnownHost::ApiKey { url, .. } => url.clone(),
            KnownHost::Basic { url, .. } => url.clone(),
            KnownHost::NoAuth { url, .. } => url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{get_secret, upsert_secret_auth};
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        let keystore = tmp.path().join("secrets.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore);
        }
        (tmp, hosts, keystore)
    }

    fn write_hosts(hosts: BTreeMap<String, KnownHost>) {
        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
    }

    #[test]
    fn roles_default_to_collect_when_omitted() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "default-role".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: Vec::new(),
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"default-role".to_string()).expect("host");
        assert!(host.has_role(HostRole::Collect));
    }

    #[test]
    fn invalid_send_role_on_kibana_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "kb-invalid-send".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::Send],
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
            },
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected invalid role error");
        assert!(err.to_string().contains("role 'send' is only valid"));
    }

    #[test]
    fn list_by_role_filters_mixed_inventory() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "collect-only".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect],
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "collect-send".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect, HostRole::Send],
                viewer: None,
                url: Url::parse("http://localhost:9201").expect("url"),
            },
        );
        hosts.insert(
            "view-host".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::View],
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
            },
        );
        write_hosts(hosts);

        let collect = KnownHost::list_by_role(HostRole::Collect).expect("collect list");
        let send = KnownHost::list_by_role(HostRole::Send).expect("send list");
        let view = KnownHost::list_by_role(HostRole::View).expect("view list");

        assert_eq!(
            collect,
            vec!["collect-only".to_string(), "collect-send".to_string()]
        );
        assert_eq!(send, vec!["collect-send".to_string()]);
        assert_eq!(view, vec!["view-host".to_string()]);
    }

    #[test]
    fn viewer_requires_send_role_on_source_host() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Collect],
                viewer: Some("viewer-host".to_string()),
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::View],
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
            },
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected viewer validation error");
        assert!(err.to_string().contains("required role 'send'"));
    }

    #[test]
    fn viewer_must_reference_view_role_host() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: Some("viewer-host".to_string()),
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::Collect],
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
            },
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected viewer role error");
        assert!(err.to_string().contains("must include role 'view'"));
    }

    #[test]
    fn viewer_reference_valid_when_source_send_and_target_view() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::NoAuth {
                app: Product::Elasticsearch,
                roles: vec![HostRole::Send],
                viewer: Some("viewer-host".to_string()),
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::NoAuth {
                app: Product::Kibana,
                roles: vec![HostRole::View],
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
            },
        );
        KnownHost::write_hosts_yml(&hosts).expect("viewer validation should pass");
    }

    #[test]
    fn legacy_hosts_auth_resolves_without_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy-es".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: Some("legacy-key".to_string()),
                app: Product::Elasticsearch,
                cloud_id: None,
                roles: default_collect_roles(),
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"legacy-es".to_string()).expect("host");
        let auth = host.get_auth().expect("auth");
        assert!(matches!(auth, Auth::Apikey(k) if k == "legacy-key"));
    }

    #[test]
    fn explicit_secret_missing_keystore_fails() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "env-key");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "secret-only".to_string(),
            KnownHost::Basic {
                accept_invalid_certs: false,
                app: Product::Elasticsearch,
                password: None,
                roles: default_collect_roles(),
                secret: Some("missing-secret".to_string()),
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
                username: None,
            },
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"secret-only".to_string()).expect("host");
        let err = host.get_auth().err().expect("auth should fail");
        assert!(err.to_string().contains("missing-secret"));
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        }
    }

    #[test]
    fn explicit_secret_takes_precedence_over_legacy_fields() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        upsert_secret_auth(
            "custom-secret",
            SecretAuth::ApiKey {
                apikey: "secret-key".to_string(),
            },
            "pw",
        )
        .expect("upsert secret");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::Basic {
                accept_invalid_certs: false,
                app: Product::Elasticsearch,
                password: Some("legacy-pass".to_string()),
                roles: default_collect_roles(),
                secret: Some("custom-secret".to_string()),
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
                username: Some("legacy-user".to_string()),
            },
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"prod-es".to_string()).expect("host");
        let auth = host.get_auth().expect("auth");
        assert!(matches!(auth, Auth::Apikey(k) if k == "secret-key"));
    }

    #[test]
    fn no_secret_uses_legacy_auth_without_keystore_lookup() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        upsert_secret_auth(
            "prod-es",
            SecretAuth::ApiKey {
                apikey: "keystore-key".to_string(),
            },
            "pw",
        )
        .expect("upsert secret");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::Basic {
                accept_invalid_certs: false,
                app: Product::Elasticsearch,
                password: Some("legacy-pass".to_string()),
                roles: default_collect_roles(),
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
                username: Some("legacy-user".to_string()),
            },
        );
        hosts.insert(
            "legacy-only".to_string(),
            KnownHost::Basic {
                accept_invalid_certs: false,
                app: Product::Elasticsearch,
                password: Some("legacy-only-pass".to_string()),
                roles: default_collect_roles(),
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9201").expect("url"),
                username: Some("legacy-only-user".to_string()),
            },
        );
        write_hosts(hosts);

        let prod_host = KnownHost::get_known(&"prod-es".to_string()).expect("host");
        let prod_auth = prod_host.get_auth().expect("auth");
        assert!(
            matches!(prod_auth, Auth::Basic(user, pass) if user == "legacy-user" && pass == "legacy-pass")
        );

        let fallback_host = KnownHost::get_known(&"legacy-only".to_string()).expect("host");
        let fallback_auth = fallback_host.get_auth().expect("auth");
        assert!(
            matches!(fallback_auth, Auth::Basic(user, pass) if user == "legacy-only-user" && pass == "legacy-only-pass")
        );
    }

    #[test]
    fn migrate_hosts_moves_legacy_credentials_to_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "es-prod".to_string(),
            KnownHost::ApiKey {
                accept_invalid_certs: false,
                apikey: Some("apikey-1".to_string()),
                app: Product::Elasticsearch,
                cloud_id: None,
                roles: default_collect_roles(),
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:9200").expect("url"),
            },
        );
        hosts.insert(
            "kb-prod".to_string(),
            KnownHost::Basic {
                accept_invalid_certs: false,
                app: Product::Kibana,
                password: Some("pass-1".to_string()),
                roles: default_collect_roles(),
                secret: None,
                viewer: None,
                url: Url::parse("http://localhost:5601").expect("url"),
                username: Some("elastic".to_string()),
            },
        );
        write_hosts(hosts);

        let (migrated, unchanged) =
            KnownHost::migrate_hosts_to_keystore("pw").expect("migrate should succeed");
        assert_eq!(migrated, 2);
        assert_eq!(unchanged, 0);

        let migrated_hosts = KnownHost::parse_hosts_yml().expect("re-read migrated hosts");
        let raw_hosts = std::fs::read_to_string(&hosts_path).expect("read hosts file");

        match migrated_hosts.get("es-prod").expect("migrated es host") {
            KnownHost::ApiKey { apikey, secret, .. } => {
                assert!(apikey.is_none(), "plaintext api key should be removed");
                assert_eq!(secret.as_deref(), Some("es-prod"));
            }
            other => panic!("expected ApiKey host after migration, got {other}"),
        }
        match migrated_hosts.get("kb-prod").expect("migrated kb host") {
            KnownHost::Basic {
                username,
                password,
                secret,
                ..
            } => {
                assert!(username.is_none(), "plaintext username should be removed");
                assert!(password.is_none(), "plaintext password should be removed");
                assert_eq!(secret.as_deref(), Some("kb-prod"));
            }
            other => panic!("expected Basic host after migration, got {other}"),
        }
        assert!(!raw_hosts.contains("apikey: apikey-1"));
        assert!(!raw_hosts.contains("username: elastic"));
        assert!(!raw_hosts.contains("password: pass-1"));

        let es_secret = get_secret("es-prod", "pw")
            .expect("get secret")
            .expect("es secret exists");
        assert_eq!(es_secret.apikey.as_deref(), Some("apikey-1"));
        assert!(es_secret.basic.is_none());

        let kb_secret = get_secret("kb-prod", "pw")
            .expect("get secret")
            .expect("kb secret exists");
        assert_eq!(
            kb_secret.basic.as_ref().map(|b| b.username.as_str()),
            Some("elastic")
        );
        assert_eq!(
            kb_secret.basic.as_ref().map(|b| b.password.as_str()),
            Some("pass-1")
        );

        let migrated_es_auth = migrated_hosts
            .get("es-prod")
            .expect("migrated es host")
            .get_auth()
            .expect("read migrated es auth");
        assert!(matches!(migrated_es_auth, Auth::Apikey(key) if key == "apikey-1"));

        let migrated_kb_auth = migrated_hosts
            .get("kb-prod")
            .expect("migrated kb host")
            .get_auth()
            .expect("read migrated kb auth");
        assert!(
            matches!(migrated_kb_auth, Auth::Basic(user, pass) if user == "elastic" && pass == "pass-1")
        );
    }
}
