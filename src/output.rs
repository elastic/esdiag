extern crate elasticsearch as es_client;
pub mod elasticsearch;
pub mod file;
pub mod stdout;
use crate::host::Host;
use crate::input::Product;
use crate::uri::Uri;
use elasticsearch::ElasticsearchClient;
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;
use url::Url;

#[derive(Debug)]
pub enum Target {
    Elasticsearch(ElasticsearchClient),
    File(PathBuf),
    Stdout,
}

#[derive(Debug)]
pub struct Output {
    pub target: Target,
    pub pretty: Option<bool>,
}

impl Output {
    pub fn new(pretty: bool) -> Self {
        Self {
            target: Target::Stdout,
            pretty: Some(pretty),
        }
    }

    pub async fn test(&self) -> Result<Value, Value> {
        let elasticsearch = match &self.target {
            Target::Elasticsearch(client) => client,
            _ => panic!("No Elasticsearch client"),
        };
        let response = match elasticsearch.test().await {
            Ok(response) => response,
            Err(e) => {
                log::error!("Failed to connect to Elasticsearch: {}", e);
                return Err(serde_json::json!({"error": e.to_string()}));
            }
        };
        match response.status_code().is_success() {
            true => {
                log::info!("Elasticsearch connection: {}", &response.status_code());
                Ok(response.json::<Value>().await.unwrap())
            }
            false => {
                log::error!("Elasticsearch connection: {}", response.status_code());
                Err(response.json::<Value>().await.unwrap())
            }
        }
    }

    pub fn from_path(filename: PathBuf) -> Self {
        Self {
            target: Target::File(filename),
            pretty: None,
        }
    }

    pub fn from_url(url: Url) -> Self {
        // create host from URL
        let host = Host::from_url(&url);
        Self {
            target: Target::Elasticsearch(ElasticsearchClient::new(host)),
            pretty: None,
        }
    }

    pub fn from_host(host: Host) -> Self {
        let app = match &host {
            Host::ApiKey { app, .. } | Host::Basic { app, .. } | Host::None { app, .. } => {
                app.clone()
            }
        };

        let target = match app {
            Product::Elasticsearch => Target::Elasticsearch(ElasticsearchClient::new(host)),
            _ => panic!("Output application can only be Elasticsearch"),
        };

        Self {
            target,
            pretty: None,
        }
    }

    pub fn from_uri(uri: Uri, pretty: bool) -> Self {
        match uri {
            Uri::Host(host) => Self::from_host(host),
            Uri::Url(url) => Self::from_url(url),
            Uri::File(filename) => Self::from_path(filename),
            Uri::Directory(dir) => panic!("Cannout output to a directory: {:?}", dir),
            Uri::Stream => Self::new(pretty),
        }
    }

    pub async fn send(&self, docs: Vec<Value>) {
        match &self.target {
            Target::Stdout => {
                stdout::print_docs(docs);
            }
            Target::File(filename) => match file::write_bulk_docs(docs, &filename) {
                Ok(_) => (),
                Err(e) => panic!("ERROR: Failed to write to file - {}", e),
            },
            Target::Elasticsearch(client) => match client.bulk_index(docs).await {
                Ok(_) => (),
                Err(e) => panic!("ERROR: Failed to index document {}", e),
            },
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Target::Elasticsearch(_) => write!(f, "elasticsearch"),
            Target::File(filename) => write!(f, "{}", filename.to_str().unwrap()),
            Target::Stdout => write!(f, "stdout"),
        }
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.target)
    }
}
