pub mod archive;
pub mod eck;
pub mod elasticsearch;
pub mod file;
pub mod kibana;
pub mod logstash;
pub mod manifest;
use crate::processor::elasticsearch::EsDataSet;
use crate::uri::Uri;
use manifest::Manifest;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, str::FromStr};

pub trait Application {
    fn get_data_sets(&self) -> Vec<DataSet>;
    fn get_lookup_sets(&self) -> Vec<DataSet>;
    fn get_metadata_sets(&self) -> Vec<DataSet>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataSet {
    Elasticsearch(EsDataSet),
    //Kibana(KbDataSet),
    //Logstash(LsDataSet),
}

impl ToString for DataSet {
    fn to_string(&self) -> String {
        match self {
            DataSet::Elasticsearch(data_set) => data_set.to_string(),
            //DataSet::Kibana(data_set) => data_set.to_string(),
            //DataSet::Logstash(data_set) => data_set.to_string(),
        }
    }
}

// Product enum to hold the Elasticsearch, Kibana, or Logstash product

#[derive(Debug, PartialEq, Hash, Clone, Eq, Serialize, Deserialize)]
pub enum Product {
    Agent,
    ECE,
    ECK,
    Elasticsearch,
    Kibana,
    Logstash,
    Unknown,
}

impl fmt::Display for Product {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Agent => write!(fmt, "Agent"),
            Self::ECE => write!(fmt, "ECE"),
            Self::ECK => write!(fmt, "ECK"),
            Self::Elasticsearch => write!(fmt, "Elasticsearch"),
            Self::Kibana => write!(fmt, "Kibana"),
            Self::Logstash => write!(fmt, "Logstash"),
            Self::Unknown => write!(fmt, "Unknown"),
        }
    }
}

impl FromStr for Product {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "ece" => Ok(Self::ECE),
            "eck" => Ok(Self::ECK),
            "es" | "elasticsearch" => Ok(Self::Elasticsearch),
            "kb" | "kibana" => Ok(Self::Kibana),
            "ls" | "logstash" => Ok(Self::Logstash),
            _ => Err(()),
        }
    }
}

impl Default for Product {
    fn default() -> Self {
        Self::Unknown
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
    fn as_path_string(&self, name: &str) -> String {
        let mut string = String::new();
        match self.subdir {
            Some(ref subdir) => {
                string.push_str(subdir);
                string.push_str("/");
            }
            None => string.push_str(""),
        }
        string.push_str(name);
        match self.extension {
            Some(ref extension) => string.push_str(extension),
            None => string.push_str(".json"),
        }
        string
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
pub struct InputDataSets {
    pub data: Vec<DataSet>,
    pub lookup: Vec<DataSet>,
    pub metadata: Vec<DataSet>,
}
impl InputDataSets {
    pub fn len(&self) -> usize {
        &self.data.len() + &self.lookup.len() + &self.metadata.len()
    }
}

impl fmt::Display for InputDataSets {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "Data: [{}], Lookup: [{}], Metadata: [{}]",
            self.data
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.lookup
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.metadata
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug)]
pub struct Input {
    pub dataset: InputDataSets,
    pub product: Product,
    pub sources: HashMap<String, Source>,
    pub uri: Uri,
    pub version: Option<Version>,
    pub manifest: Manifest,
}

impl Input {
    pub fn new(uri: Uri, manifest: Manifest) -> Self {
        let application = match &manifest.product {
            Product::Agent => todo!("Elastic Agent"),
            Product::ECE => todo!("Elasitc Cloud Enterprise (ECE)"),
            Product::ECK => eck::ElasticCloudKubernetes::new(),
            Product::Elasticsearch => elasticsearch::Elasticsearch::new(),
            Product::Kibana => kibana::Kibana::new(),
            Product::Logstash => logstash::Logstash::new(),
            Product::Unknown => panic!("Cannot import an unknown product!"),
        };
        let sources = match file::parse_sources_yml(&manifest.product) {
            Ok(sources) => sources,
            Err(e) => panic!("Error parsing sources file: {}", e),
        };
        let version = match &manifest.product_version {
            Some(product_version) => Version::new(
                product_version.major,
                product_version.minor,
                product_version.patch,
            ),
            None => Version::new(0, 0, 0),
        };

        Self {
            product: manifest.product.clone(),
            dataset: InputDataSets {
                data: application.get_data_sets(),
                lookup: application.get_lookup_sets(),
                metadata: application.get_metadata_sets(),
            },
            manifest,
            uri,
            sources,
            version: Some(version),
        }
    }

    pub fn load_string(&self, dataset: &DataSet) -> Option<String> {
        let name = dataset.to_string();
        let source = match self.sources.get(&name) {
            Some(source) => source,
            None => panic!("ERROR: Source not found for {name}"),
        };
        match &self.uri {
            Uri::Directory(dir) => {
                match file::read_string(&dir.with_file_name(&source.as_path_string(&name))) {
                    Ok(string) => Some(string),
                    Err(e) => {
                        log::debug!("Error reading file '{:?}'", e);
                        None
                    }
                }
            }
            Uri::File(file) => match archive::read_string(file, &source.as_path_string(&name)) {
                Ok(string) => Some(string),
                Err(e) => {
                    log::debug!("Error reading file '{:?}'", e);
                    None
                }
            },
            _ => {
                unimplemented!("Only Directory and File input types implemented!");
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
