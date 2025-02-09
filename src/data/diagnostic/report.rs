use super::{DiagnosticManifest, DiagnosticMetadata, Lookup, Product};
use color_eyre::eyre::{eyre, OptionExt, Report, Result};
use serde::Serialize;
use std::collections::HashMap;

pub struct DiagnosticReportBuilder {
    lookups: HashMap<String, LookupSummary>,
    processors: HashMap<String, ProcessorSummary>,
    product: Option<Product>,
    docs_total: u32,
    metadata: DiagnosticMetadata,
    origin: Option<Origin>,
}

impl DiagnosticReportBuilder {
    pub fn build(self) -> Result<DiagnosticReport> {
        DiagnosticReport::try_from(self)
    }

    pub fn product(self, product: Product) -> Self {
        Self {
            product: Some(product),
            ..self
        }
    }

    pub fn receiver(self, receiver: String) -> Self {
        Self {
            origin: Origin::try_from(receiver).ok(),
            ..self
        }
    }
}

impl From<DiagnosticMetadata> for DiagnosticReportBuilder {
    fn from(metadata: DiagnosticMetadata) -> Self {
        Self {
            docs_total: 0,
            lookups: HashMap::new(),
            metadata,
            processors: HashMap::new(),
            product: None,
            origin: None,
        }
    }
}

impl TryFrom<DiagnosticManifest> for DiagnosticReportBuilder {
    type Error = color_eyre::eyre::Report;

    fn try_from(manifest: DiagnosticManifest) -> Result<Self> {
        let metadata = DiagnosticMetadata::try_from(manifest)?;
        Ok(Self {
            docs_total: 0,
            lookups: HashMap::new(),
            metadata,
            processors: HashMap::new(),
            product: None,
            origin: None,
        })
    }
}

#[derive(Serialize, Clone)]
pub struct DiagnosticReport {
    product: Product,
    origin: Origin,
    pub docs_total: u32,
    lookups: HashMap<String, LookupSummary>,
    processors: HashMap<String, ProcessorSummary>,
    #[serde(flatten)]
    pub metadata: DiagnosticMetadata,
}

impl DiagnosticReport {
    pub fn add_processor_summary(&mut self, summary: ProcessorSummary) {
        self.docs_total += summary.docs;
        self.processors.insert(summary.processor.clone(), summary);
    }

    pub fn add_lookup<T>(&mut self, name: &str, lookup: &Lookup<T>)
    where
        T: Clone + Serialize,
    {
        let summary = LookupSummary {
            count: lookup.len() as u32,
        };
        self.lookups.insert(name.to_string(), summary);
    }
}

impl TryFrom<DiagnosticReportBuilder> for DiagnosticReport {
    type Error = color_eyre::eyre::Report;

    fn try_from(builder: DiagnosticReportBuilder) -> Result<Self> {
        Ok(Self {
            docs_total: builder.docs_total,
            lookups: builder.lookups,
            metadata: builder.metadata,
            origin: builder.origin.ok_or_else(|| eyre!("Origin not set"))?,
            processors: builder.processors,
            product: builder.product.unwrap_or(Product::Unknown),
        })
    }
}

#[derive(Serialize, Clone)]
pub struct BatchResponse {
    pub docs: u32,
    pub errors: u32,
    pub retries: u16,
    pub size: u32,
    pub status_code: u16,
    pub time: u32,
}

impl BatchResponse {
    pub fn new(docs: u32) -> Self {
        Self {
            docs,
            errors: 0,
            retries: 0,
            size: 0,
            status_code: 0,
            time: 0,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ProcessorSummary {
    batch: BatchStats,
    pub docs: u32,
    doc_errors: u32,
    #[serde(skip_serializing)]
    pub processor: String,
    index: String,
    pub source: Source,
}

#[derive(Serialize, Clone)]
pub struct BatchStats {
    count: u32,
    errors: u32,
    retries: u16,
    status_codes: HashMap<u16, u32>,
    #[serde(skip_serializing)]
    pub responses: Vec<BatchResponse>,
}

#[derive(Serialize, Clone)]
pub struct Source {
    pub parsed: bool,
}

impl ProcessorSummary {
    pub fn new(name: String) -> Self {
        Self {
            batch: BatchStats {
                count: 0,
                errors: 0,
                retries: 0,
                status_codes: HashMap::new(),
                responses: Vec::new(),
            },
            index: name.clone(),
            docs: 0,
            doc_errors: 0,
            processor: name,
            source: Source { parsed: false },
        }
    }

    pub fn add_batch(&mut self, batch: BatchResponse) {
        self.batch.count += 1;
        self.batch.errors += batch.errors;
        self.batch.retries += batch.retries;
        self.batch
            .status_codes
            .entry(batch.status_code)
            .and_modify(|count| *count += 1)
            .or_insert(1);
        self.docs += batch.docs;
        self.batch.responses.push(batch);
    }

    pub fn was_parsed(self) -> Self {
        Self {
            source: Source { parsed: true },
            ..self
        }
    }

    pub fn rename(self, name: String) -> Self {
        Self {
            processor: name,
            ..self
        }
    }
}

#[derive(Serialize, Clone)]
pub struct LookupSummary {
    count: u32,
}

#[derive(Serialize, Clone)]
struct Origin {
    r#type: String,
    path: String,
}

impl TryFrom<String> for Origin {
    type Error = Report;

    fn try_from(receiver: String) -> Result<Self> {
        let (r#type, path) = receiver
            .split_once(' ')
            .ok_or_eyre("Invalid receiver string")?;
        Ok(Self {
            r#type: r#type.to_string(),
            path: path.to_string(),
        })
    }
}
