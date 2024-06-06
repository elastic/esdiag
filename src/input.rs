pub mod elasticsearch;
pub mod file;
pub mod kibana;
pub mod logstash;
pub mod manifest;
use crate::processor::{elasticsearch::EsDataSet, kibana::KbDataSet, logstash::LsDataSet};
use crate::uri::Uri;
use manifest::Manifest;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt, path::PathBuf, str::FromStr};

pub trait Application {
    fn get_metadata_sets(&self) -> Vec<DataSet>;
    fn get_data_sets(&self) -> Vec<DataSet>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataSet {
    Elasticsearch(EsDataSet),
    Kibana(KbDataSet),
    Logstash(LsDataSet),
}

impl ToString for DataSet {
    fn to_string(&self) -> String {
        match self {
            DataSet::Elasticsearch(data_set) => data_set.to_string(),
            DataSet::Kibana(data_set) => data_set.to_string(),
            DataSet::Logstash(data_set) => data_set.to_string(),
        }
    }
}

// Product enum to hold the Elasticsearch, Kibana, or Logstash product

#[derive(Debug, PartialEq, Hash, Clone, Eq, Serialize, Deserialize)]
pub enum Product {
    Elasticsearch,
    Kibana,
    Logstash,
}

impl fmt::Display for Product {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::Logstash => write!(fmt, "Logstash"),
        }
    }
}

impl FromStr for Product {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            _ => Err(()),
        }
    }
}

impl Default for Product {
    fn default() -> Self {
        Self::Elasticsearch
    }
}

// Source struct to hold the name, extension, subdir, and versions of the source

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub struct Source {
    pub extension: Option<String>,
    pub subdir: Option<String>,
    pub versions: HashMap<String, String>,
}

impl Source {
    pub fn with_dir(&self, name: &str, dir: &PathBuf) -> PathBuf {
        let mut path: PathBuf = PathBuf::new();
        path.push(&dir);
        match &self.subdir {
            Some(subdir) => path.push(subdir),
            None => (),
        }
        let filename = match &self.extension {
            Some(extension) => format!("{}{}", name, extension),
            None => format!("{}.json", name),
        };
        path.push(filename);
        path
    }

    //pub fn with_url(
    //    &self,
    //    url: &Url,
    //    version: &Version,
    //) -> Result<Url, Box<dyn std::error::Error>> {
    //    for (version_req, path) in self.versions.iter() {
    //        let version_req: VersionReq = VersionReq::parse(version_req)?;
    //        if version_req.matches(version) {
    //            let s: String = url.to_string() + &path;
    //            return Ok(Url::parse(&s).unwrap());
    //        }
    //    }
    //    Err("ERROR: No matching version found for source".into())
    //}
}

impl Default for Source {
    fn default() -> Self {
        Self {
            extension: Some(String::from(".json")),
            subdir: None,
            versions: HashMap::new(),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", self)
    }
}

// Input struct to hold the product, sources, and version

#[derive(Debug)]
pub struct Input {
    pub data_sets: Vec<DataSet>,
    pub metadata_sets: Vec<DataSet>,
    pub product: Product,
    pub sources: HashMap<String, Source>,
    pub uri: Uri,
    pub version: Option<Version>,
}

impl Input {
    pub fn new(uri: Uri, manifest: &Manifest) -> Self {
        let application = match manifest.product {
            Product::Elasticsearch => elasticsearch::Elasticsearch::new(),
            Product::Kibana => kibana::Kibana::new(),
            Product::Logstash => logstash::Logstash::new(),
        };
        let sources = match file::parse_sources_yml(&manifest.product) {
            Ok(sources) => sources,
            Err(e) => panic!("ERROR: Failed to parse sources file - {}", e),
        };
        let version = Version::new(
            manifest.product_version.major,
            manifest.product_version.minor,
            manifest.product_version.patch,
        );

        Self {
            product: manifest.product.clone(),
            metadata_sets: application.get_metadata_sets(),
            data_sets: application.get_data_sets(),
            uri,
            sources,
            version: Some(version),
        }
    }

    pub fn load(&self, dataset: &DataSet) -> Value {
        let key = dataset.to_string();
        let source = match self.sources.get(&key) {
            Some(source) => source,
            None => panic!("ERROR: Source not found for {key}"),
        };
        match &self.uri {
            Uri::Directory(dir) => match file::parse_json(&source.with_dir(&key, dir)) {
                Ok(json) => json,
                Err(_) => match file::read_first_line(&source.with_dir(&key, dir)) {
                    Ok(line) => {
                        log::error!(
                            "Failed to parse {}, contains: \"{}\"",
                            source.with_dir(&key, dir).to_str().unwrap(),
                            &line
                        );
                        panic!("File did not have valid json");
                    }
                    Err(e) => panic!("Failed to read file - {}", e),
                },
            },
            _ => {
                unimplemented!("Input type no implemented!");
            }
        }
    }
}

impl fmt::Display for Input {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "Processing {} version {} from {:?}",
            self.product,
            self.version.clone().unwrap(),
            self.uri,
        )
    }
}
