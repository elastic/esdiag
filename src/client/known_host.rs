use crate::data::diagnostic::Product;
use color_eyre::eyre::{eyre, Result};
use reqwest;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "auth")]
pub enum KnownHost {
    /// A host using API key authentication
    ApiKey {
        accept_invalid_certs: Option<bool>,
        apikey: String,
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<String>,
        url: Url,
    },
    /// A host using basic username/password authentication
    Basic {
        accept_invalid_certs: Option<bool>,
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<String>,
        password: String,
        url: Url,
        username: String,
    },
    /// A host with no authentication
    None { app: Product, url: Url },
}

impl KnownHost {
    pub fn try_new(
        url: Url,
        app: String,
        accept_invalid_certs: bool,
        apikey: Option<String>,
        cloud_id: Option<String>,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self> {
        match (apikey, username, password) {
            (Some(apikey), None, None) => Ok(KnownHost::ApiKey {
                apikey,
                app: Product::from_str(&app).expect("A valid application is required!"),
                accept_invalid_certs: Some(accept_invalid_certs),
                cloud_id,
                url,
            }),
            (None, Some(username), Some(password)) => Ok(KnownHost::Basic {
                app: Product::from_str(&app).expect("A valid application is required!"),
                accept_invalid_certs: Some(accept_invalid_certs),
                cloud_id,
                password,
                url,
                username,
            }),
            (None, None, None) => Ok(KnownHost::None {
                app: Product::from_str(&app).expect("A valid application is required!"),
                url,
            }),
            _ => Err(eyre!("Invalid combination of authentication properties")),
        }
    }

    pub fn get_url(&self) -> Url {
        match self {
            Self::ApiKey { url, .. } => url.clone(),
            Self::Basic { url, .. } => url.clone(),
            Self::None { url, .. } => url.clone(),
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
            Self::None { .. } => {
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
        KnownHost::None {
            app: Product::Elasticsearch,
            url: url.clone(),
        }
    }

    async fn validate_application(&self, response: reqwest::Response) -> (bool, String) {
        let status = response.status();
        let body = response.text().await.expect("Failed to read test body");
        let json = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        let app = match self {
            Self::ApiKey { app, .. } | Self::Basic { app, .. } | Self::None { app, .. } => app,
        };
        match app {
            Product::Elasticsearch => match json.get("tagline") {
                Some(_) => (true, format!("{} ✅ Elasticsearch", status)),
                None => (
                    false,
                    format!(
                        "{} ❌ No tagline? Host is not an Elasticsearch cluster!",
                        status
                    ),
                ),
            },
            _ => (false, format!("{} ⛔️ Unsupported application", status)),
        }
    }

    pub async fn test(&self) -> Result<(bool, String), reqwest::Error> {
        match self {
            Self::ApiKey {
                apikey,
                app,
                accept_invalid_certs,
                cloud_id: _,
                url,
            } => {
                // test the connection
                log::info!("Testing {} connection", &app);
                // create a client with the API key
                let client = reqwest::Client::builder()
                    .default_headers(
                        std::iter::once((
                            reqwest::header::AUTHORIZATION,
                            format!("ApiKey {}", apikey)
                                .parse()
                                .expect("Failed to parse apikey"),
                        ))
                        .collect(),
                    )
                    .danger_accept_invalid_certs(accept_invalid_certs.unwrap_or(false))
                    .build()?;
                log::trace!("Reqwest client: {:?}", client);
                let response = client.get(url.as_str()).send().await;
                match response {
                    Ok(response) => Ok(self.validate_application(response).await),
                    Err(e) => Err(e),
                }
            }
            Self::Basic {
                app,
                accept_invalid_certs,
                cloud_id: _,
                password,
                url,
                username,
            } => {
                // test the connection
                log::info!("Testing {} connection", &app);
                let client = reqwest::Client::builder()
                    .danger_accept_invalid_certs(accept_invalid_certs.unwrap_or(false))
                    .build()?;
                let response = client
                    .get(url.as_str())
                    .basic_auth(username, Some(password))
                    .send()
                    .await;
                match response {
                    Ok(response) => Ok(self.validate_application(response).await),
                    Err(e) => Err(e),
                }
            }
            Self::None { app, url } => {
                // test the connection
                log::info!("Testing {} connection", &app);
                let response = reqwest::get(url.as_str()).await;
                match response {
                    Ok(response) => Ok(self.validate_application(response).await),
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub fn get_hosts_path() -> PathBuf {
        match env::var("ESDIAG_HOSTS") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                let home = match env::var("HOME") {
                    Ok(home) => PathBuf::from(home),
                    Err(_) => panic!("ERROR: No home directory found"),
                };
                // Check if the `.esdiag` directory exists, if not, create it
                let esdiag = home.join(".esdiag");
                if !esdiag.exists() {
                    std::fs::create_dir(&esdiag).expect("Failed to create ~/.esdiag directory");
                }
                let path = home.join(".esdiag").join("hosts.yml");
                path
            }
        }
    }

    /// loads hosts from the resources directory
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
            } => write!(
                fmt,
                "Host ApiKey: {} {} {}",
                app,
                url,
                cloud_id.as_deref().unwrap_or(""),
            ),
            Self::Basic {
                app,
                cloud_id,
                url,
                username,
                ..
            } => write!(
                fmt,
                "Host Basic: {} {}@ {} {}",
                app,
                username,
                url,
                cloud_id.as_deref().unwrap_or(""),
            ),
            Self::None { app, url } => write!(fmt, "Host None: {} {}", app, url),
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
