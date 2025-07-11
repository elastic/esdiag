use super::{DiagnosticManifest, DiagnosticMetadata, Lookup, Product};
use eyre::{OptionExt, Report, Result, eyre};
use serde::Serialize;
use std::collections::HashMap;

pub struct DiagnosticReportBuilder {
    processors: HashMap<String, ProcessorSummary>,
    product: Option<Product>,
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
            metadata,
            processors: HashMap::new(),
            product: None,
            origin: None,
        }
    }
}

impl TryFrom<DiagnosticManifest> for DiagnosticReportBuilder {
    type Error = eyre::Report;

    fn try_from(manifest: DiagnosticManifest) -> Result<Self> {
        let metadata = DiagnosticMetadata::try_from(manifest)?;
        Ok(Self {
            metadata,
            processors: HashMap::new(),
            product: None,
            origin: None,
        })
    }
}

#[derive(Serialize, Clone)]
pub struct Identifiers {
    pub account: Option<String>,
    pub case: Option<String>,
    pub filename: Option<String>,
    pub opportunity: Option<String>,
    pub user: Option<String>,
}

impl Default for Identifiers {
    fn default() -> Self {
        Self {
            account: None,
            case: None,
            filename: None,
            opportunity: None,
            user: None,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct DiagnosticReport {
    pub product: Product,
    origin: Origin,
    pub docs: Docs,
    lookup: NestedStats<LookupSummary>,
    processor: NestedStats<ProcessorSummary>,
    #[serde(flatten)]
    pub metadata: DiagnosticMetadata,
    pub kibana_link: Option<String>,
    #[serde(flatten)]
    pub identifiers: Identifiers,
}

impl DiagnosticReport {
    pub fn add_kibana_link(&mut self, link: String) {
        self.kibana_link = Some(link);
    }

    pub fn add_identifiers(&mut self, identifiers: Identifiers) {
        self.identifiers = identifiers;
    }
}

#[derive(Serialize, Clone)]
pub struct Docs {
    pub created: u32,
    pub errors: u32,
    pub total: u32,
}
#[derive(Serialize, Clone)]
struct NestedStats<T> {
    count: u32,
    errors: u32,
    failures: Vec<String>,
    stats: HashMap<String, T>,
}

impl<T> NestedStats<T> {
    fn push(&mut self, name: String, summary: T) {
        self.count += 1;
        self.stats.insert(name, summary);
    }
}

impl DiagnosticReport {
    pub fn add_processor_summary(&mut self, summary: ProcessorSummary) {
        if !summary.source.parsed {
            self.processor.errors += 1;
            self.processor.failures.push(summary.index.clone());
        }
        self.docs.created += summary.docs;
        self.docs.errors += summary.doc_errors;
        self.docs.total += summary.docs + summary.doc_errors;
        self.processor.push(summary.processor.clone(), summary);
    }

    pub fn add_lookup<T>(&mut self, name: &str, lookup: &Lookup<T>)
    where
        T: Clone + Serialize,
    {
        self.lookup.push(
            name.to_string(),
            LookupSummary {
                docs: lookup.len() as u32,
            },
        );
    }
}

impl TryFrom<DiagnosticReportBuilder> for DiagnosticReport {
    type Error = eyre::Report;

    fn try_from(builder: DiagnosticReportBuilder) -> Result<Self> {
        Ok(Self {
            docs: Docs {
                created: 0,
                errors: 0,
                total: 0,
            },
            lookup: NestedStats::<LookupSummary> {
                count: 0,
                errors: 0,
                failures: Vec::new(),
                stats: HashMap::<String, LookupSummary>::new(),
            },
            metadata: builder.metadata,
            origin: builder.origin.ok_or_else(|| eyre!("Origin not set"))?,
            processor: NestedStats::<ProcessorSummary> {
                count: 0,
                errors: 0,
                failures: Vec::new(),
                stats: builder.processors,
            },
            product: builder.product.unwrap_or(Product::Unknown),
            kibana_link: None,
            identifiers: Identifiers::default(),
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
pub struct LookupSummary {
    docs: u32,
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
        self.batch.retries += batch.retries;
        self.batch
            .status_codes
            .entry(batch.status_code)
            .and_modify(|count| *count += 1)
            .or_insert(1);
        self.docs += batch.docs;
        self.doc_errors += batch.errors;
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
