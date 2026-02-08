// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::data::{Auth, Product};
use eyre::{eyre, Result};
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
    url: Url,
    username: Option<String>,
}

impl KnownHostBuilder {
    pub fn new(url: Url) -> Self {
        KnownHostBuilder {
            accept_invalid_certs: false,
            apikey: None,
            product: Product::Elasticsearch,
            cloud_id: None,
            password: None,
            url,
            username: None,
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

    pub fn username(self, username: Option<String>) -> Self {
        Self { username, ..self }
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
                    Some(domain) if domain == "admin.found.no" => "main-elasticsearch",
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

        log::debug!("Updated Cloud API URL: {}", url);
        self.url = url;
    }

    pub fn build(mut self) -> Result<KnownHost> {
        self.update_cloud_api_path();
        match (self.apikey, self.username, self.password) {
            (Some(apikey), None, None) => Ok(KnownHost::ApiKey {
                accept_invalid_certs: self.accept_invalid_certs,
                apikey,
                app: self.product,
                cloud_id: self.cloud_id,
                url: self.url,
            }),
            (None, Some(username), Some(password)) => Ok(KnownHost::Basic {
                accept_invalid_certs: self.accept_invalid_certs,
                app: self.product,
                password,
                url: self.url,
                username,
            }),
            (None, None, None) => Ok(KnownHost::NoAuth {
                app: self.product,
                url: self.url,
            }),
            _ => Err(eyre!("Invalid KnownHost configuration")),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "auth")]
pub enum KnownHost {
    /// A host using API key authentication
    ApiKey {
        accept_invalid_certs: bool,
        apikey: String,
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<ElasticCloud>,
        url: Url,
    },
    /// A host using basic username/password authentication
    Basic {
        accept_invalid_certs: bool,
        app: Product,
        password: String,
        url: Url,
        username: String,
    },
    /// A host with no authentication
    #[serde(alias = "None")]
    NoAuth { app: Product, url: Url },
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

    pub fn get_auth(&self) -> Auth {
        match self {
            Self::ApiKey { apikey, .. } => Auth::Apikey(apikey.clone()),
            Self::Basic {
                username, password, ..
            } => Auth::Basic(username.clone(), password.clone()),
            Self::NoAuth { .. } => Auth::None,
        }
    }

    pub fn save(self, name: &String) -> Result<String> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let mut hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                log::error!("Error parsing hosts.yml: {}", e);
                return Err(eyre!("Error parsing hosts.yml"));
            }
        };
        match self {
            Self::ApiKey { .. } => {
                hosts.insert(name.clone(), self);
            }
            Self::Basic { .. } => {
                hosts.insert(name.clone(), self);
            }
            Self::NoAuth { .. } => {
                hosts.insert(name.clone(), self);
            }
        }
        KnownHost::write_hosts_yml(&hosts)
    }

    pub fn get_known(host: &String) -> Option<Self> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                log::error!("Error parsing hosts.yml: {}", e);
                return None;
            }
        };
        log::debug!(
            "Known hosts: {}",
            hosts
                .clone()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<String>>()
                .join(", ")
        );
        hosts.get(host).cloned()
    }

    pub fn from_url(url: &Url) -> Self {
        KnownHost::NoAuth {
            app: Product::Elasticsearch,
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
                let path = home_dir.join(".esdiag").join("hosts.yml");
                path
            }
        }
    }

    /// Loads hosts from the ~/.esdiag/hosts.yml (defalt) file
    pub fn parse_hosts_yml() -> Result<BTreeMap<String, KnownHost>> {
        let path = KnownHost::get_hosts_path();
        log::debug!("Parsing {:?}", path);
        match path.is_file() {
            true => {
                let file = File::open(path)?;
                let reader = BufReader::new(file);
                let hosts: BTreeMap<String, KnownHost> = serde_yaml::from_reader(reader)?;
                Ok(hosts)
            }
            false => {
                log::info!("No hosts, file creating {:?}", path);
                File::create(path)?;
                Ok(BTreeMap::new())
            }
        }
    }

    pub fn write_hosts_yml(hosts: &BTreeMap<String, KnownHost>) -> Result<String> {
        let path = KnownHost::get_hosts_path();
        log::debug!(
            "Writing hosts: {} to {:?}",
            hosts
                .clone()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<String>>()
                .join(", "),
            path
        );
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        serde_yaml::to_writer(writer, hosts)?;
        Ok(format!("{}", &path.display()))
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
                write!(fmt, "KnownHost Basic: {} {}@ {}", app, username, url,)
            }
            Self::NoAuth { app, url } => write!(fmt, "KnownHost NoAuth: {} {}", app, url),
        }
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
