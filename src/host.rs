use crate::input::Product;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::BTreeMap;
use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::str::FromStr;
use url::Url;

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
pub fn parse_hosts_yml() -> Result<BTreeMap<String, Host>, Box<dyn std::error::Error>> {
    let path = get_hosts_path();
    log::debug!("Parsing {:?}", path);
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let hosts: Result<BTreeMap<String, Host>, serde_yaml::Error> = serde_yaml::from_reader(reader);
    Ok(hosts?)
}

pub fn write_hosts_yml(hosts: &BTreeMap<String, Host>) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_hosts_path();
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
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let hosts = serde_yaml::to_writer(writer, hosts);
    Ok(hosts?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "auth")]
pub enum Host {
    ApiKey {
        apikey: String,
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<String>,
        url: Url,
    },
    Basic {
        app: Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: Option<String>,
        password: String,
        url: Url,
        username: String,
    },
    None {
        app: Product,
        url: Url,
    },
}

impl Host {
    pub fn new(
        url: Url,
        app: String,
        auth: String,
        apikey: Option<String>,
        cloud_id: Option<String>,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        match Auth::from_str(&auth) {
            Ok(Auth::ApiKey) => Host::ApiKey {
                apikey: apikey.expect("ApiKey auth requires an API key!"),
                app: Product::from_str(&app).expect("A valid application is required!"),
                cloud_id,
                url,
            },
            Ok(Auth::Basic) => Host::Basic {
                app: Product::from_str(&app).expect("A valid application is required!"),
                cloud_id,
                password: password.expect("Basic auth requires a password!"),
                url,
                username: username.expect("Basic auth requires a username!"),
            },
            Ok(Auth::None) => Host::None {
                app: Product::from_str(&app).expect("A valid application is required!"),
                url,
            },
            Err(_) => panic!("Invalid auth type: {}", auth),
        }
    }

    pub fn save(self, name: String) -> Result<(), std::io::Error> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let mut hosts = match parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                log::error!("Error parsing hosts.yml: {}", e);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Error parsing hosts.yml",
                ));
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
        match write_hosts_yml(&hosts) {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Error writing hosts.yml: {}", e);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Error writing hosts.yml",
                ));
            }
        }
    }

    pub fn get_known(host: &String) -> Option<Self> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let hosts = match parse_hosts_yml() {
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
        Host::None {
            app: Product::Elasticsearch,
            url: url.clone(),
        }
    }

    pub async fn test(&self) -> Result<reqwest::Response, reqwest::Error> {
        match self {
            Self::ApiKey {
                apikey,
                app,
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
                    .build()?;
                log::debug!("Reqwest client: {:?}", client);
                client.get(url.as_str()).send().await
            }
            Self::Basic {
                app,
                cloud_id: _,
                password,
                url,
                username,
            } => {
                // test the connection
                log::info!("Testing {} connection", &app);
                let client = reqwest::Client::new();
                client
                    .get(url.as_str())
                    .basic_auth(username, Some(password))
                    .send()
                    .await
            }
            Self::None { app, url } => {
                // test the connection
                log::info!("Testing {} connection", &app);
                reqwest::get(url.as_str()).await
            }
        }
    }
}

impl Display for Host {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKey {
                apikey,
                app,
                cloud_id,
                url,
            } => write!(
                fmt,
                "Host: {} {} {} {}",
                app,
                url,
                cloud_id.as_deref().unwrap_or(""),
                apikey
            ),
            Self::Basic {
                app,
                cloud_id,
                password,
                url,
                username,
            } => write!(
                fmt,
                "Host: {} {} {} {} {}",
                app,
                url,
                cloud_id.as_deref().unwrap_or(""),
                username,
                password
            ),
            Self::None { app, url } => write!(fmt, "Host: {} {}", app, url),
        }
    }
}

impl FromStr for Host {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Host::get_known(&s.to_string()) {
            Some(host) => Ok(host),
            None => Err(()),
        }
    }
}

enum Auth {
    ApiKey,
    Basic,
    None,
}

impl FromStr for Auth {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "apikey" => Ok(Self::ApiKey),
            "basic" => Ok(Self::Basic),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}
