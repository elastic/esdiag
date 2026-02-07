// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::elasticsearch::{ClusterMetadata, License as ElasticsearchLicense};
use super::{DiagnosticManifest, DiagnosticMetadata, Lookup};
use crate::data::Product;
use eyre::{eyre, OptionExt, Report, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct DiagnosticReportBuilder {
    cluster: Option<ClusterMetadata>,
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

    pub fn cluster(self, cluster: ClusterMetadata) -> Self {
        Self {
            cluster: Some(cluster),
            ..self
        }
    }
}

impl From<DiagnosticMetadata> for DiagnosticReportBuilder {
    fn from(metadata: DiagnosticMetadata) -> Self {
        Self {
            metadata,
            cluster: None,
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
            cluster: None,
            metadata,
            processors: HashMap::new(),
            product: None,
            origin: None,
        })
    }
}

/// Identifiers associated with a diagnostic report
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identifiers {
    /// Account identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Case number associated with the diagnostic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_number: Option<String>,
    /// Filename of the diagnostic bundle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Opportunity identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opportunity: Option<String>,
    /// User who generated the diagnostic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Parent diagnostic identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Orchestration platform
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<String>,
}
impl Identifiers {
    pub fn new(
        account: Option<String>,
        case_number: Option<String>,
        filename: Option<String>,
        opportunity: Option<String>,
        user: Option<String>,
    ) -> Self {
        Self {
            account,
            case_number,
            filename,
            opportunity,
            user: user.or_else(|| std::env::var("ESDIAG_USER").ok()),
            parent_id: None,
            orchestration: None,
        }
    }

    pub fn default_user(self, username: Option<&String>) -> Self {
        Self {
            user: self.user.or_else(|| username.cloned()),
            ..self
        }
    }

    pub fn filename_as_str(&self) -> &str {
        self.filename.as_deref().unwrap_or("none")
    }

    pub fn with_filename(self, filename: Option<String>) -> Self {
        Self { filename, ..self }
    }

    pub fn with_parent_id(self, parent_id: String) -> Self {
        Self {
            parent_id: Some(parent_id),
            ..self
        }
    }

    pub fn with_orchestration(self, orchestration: String) -> Self {
        Self {
            orchestration: Some(orchestration),
            ..self
        }
    }
}

impl Default for Identifiers {
    fn default() -> Self {
        let user = std::env::var("ESDIAG_USER").ok();
        Self {
            account: None,
            case_number: None,
            filename: None,
            opportunity: None,
            user,
            parent_id: None,
            orchestration: None,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(untagged)]
pub enum License {
    Elasticsearch(ElasticsearchLicense),
}

#[derive(Serialize)]
pub struct DiagnosticReport {
    #[serde(rename = "@timestamp")]
    timestamp: i64,
    cluster: Option<ClusterMetadata>,
    pub diagnostic: DiagnosticStats,
    agent: Agent,
}

#[derive(Serialize)]
pub struct Agent {
    pub r#type: &'static str,
    pub version: semver::Version,
}

#[derive(Serialize)]
pub struct DiagnosticStats {
    pub docs: Docs,
    pub product: Product,
    origin: Origin,
    pub license: Option<License>,
    lookup: NestedStats<LookupSummary>,
    processor: NestedStats<ProcessorSummary>,
    #[serde(flatten)]
    pub metadata: DiagnosticMetadata,
    pub kibana_link: Option<String>,
    #[serde(flatten)]
    pub identifiers: Identifiers,
    pub processing_duration: u128,
}

impl DiagnosticReport {
    pub fn add_kibana_link(&mut self, link: String) {
        self.diagnostic.kibana_link = Some(link);
    }

    pub fn add_identifiers(&mut self, identifiers: Identifiers) {
        self.diagnostic.identifiers = identifiers;
    }

    pub fn add_license(&mut self, license: Option<ElasticsearchLicense>) {
        self.diagnostic.license = license.map(License::Elasticsearch);
    }

    pub fn add_processing_duration(&mut self, time: u128) {
        self.diagnostic.processing_duration = time;
    }

    pub fn add_origin(&mut self, (name, id, scope): (String, String, String)) {
        self.diagnostic.origin.name = Some(name);
        self.diagnostic.origin.id = Some(id);
        self.diagnostic.origin.scope = Some(scope);
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
            self.diagnostic.processor.errors += 1;
            self.diagnostic
                .processor
                .failures
                .push(summary.index.clone());
        }
        self.diagnostic.docs.created += summary.docs;
        self.diagnostic.docs.errors += summary.doc_errors;
        self.diagnostic.docs.total += summary.docs + summary.doc_errors;
        self.diagnostic
            .processor
            .push(summary.processor.clone(), summary);
    }

    pub fn add_lookup<T>(&mut self, name: &str, lookup: &Lookup<T>)
    where
        T: Clone + Serialize,
    {
        if !lookup.parsed && !self.diagnostic.lookup.failures.iter().any(|f| f == name) {
            self.diagnostic.lookup.errors += 1;
            self.diagnostic.lookup.failures.push(name.to_string());
        }

        self.diagnostic.lookup.push(
            name.to_string(),
            LookupSummary {
                docs: lookup.len() as u32,
                parsed: lookup.parsed,
            },
        );
    }
}

impl TryFrom<DiagnosticReportBuilder> for DiagnosticReport {
    type Error = eyre::Report;

    fn try_from(builder: DiagnosticReportBuilder) -> Result<Self> {
        Ok(Self {
            agent: Agent {
                r#type: "esdiag",
                version: semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
            },
            timestamp: chrono::Utc::now().timestamp_millis(),
            cluster: builder.cluster,
            diagnostic: DiagnosticStats {
                docs: Docs {
                    created: 0,
                    errors: 0,
                    total: 0,
                },
                license: None,
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
                processing_duration: 0,
            },
        })
    }
}

#[derive(Serialize, Clone, Copy)]
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

impl std::fmt::Display for BatchResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Batch: {:>8} docs {:>8} errors {:>8} retries {:>8} size http-{} time: {:>10}",
            self.docs, self.errors, self.retries, self.size, self.status_code, self.time
        )
    }
}

#[derive(Serialize, Clone)]
pub struct LookupSummary {
    docs: u32,
    pub parsed: bool,
}

#[derive(Serialize, Clone)]
pub struct ProcessorSummary {
    batch: BatchStats,
    pub docs: u32,
    doc_errors: u32,
    #[serde(skip_serializing)]
    pub processor: String,
    pub index: String,
    pub source: Source,
}

impl ProcessorSummary {
    pub fn merge(&mut self, other: Result<ProcessorSummary>) {
        match other {
            Ok(other) => {
                self.batch.merge(other.batch);
                self.docs += other.docs;
                self.doc_errors += other.doc_errors;
            }
            Err(err) => {
                log::warn!("processor summary was err: {}", err);
            }
        }
    }
}

impl std::fmt::Display for ProcessorSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Processor: {:<30} {}\t{:>7} docs {:>7} errors",
            self.processor, self.source, self.docs, self.doc_errors
        )
    }
}

#[derive(Serialize, Clone)]
pub struct BatchStats {
    count: u32,
    retries: u16,
    status_codes: HashMap<u16, u32>,
    #[serde(skip_serializing)]
    pub responses: Vec<BatchResponse>,
}

impl BatchStats {
    pub fn merge(&mut self, other: BatchStats) {
        self.count += other.count;
        self.retries += other.retries;
        for (code, count) in other.status_codes {
            self.status_codes
                .entry(code)
                .and_modify(|c| *c += count)
                .or_insert(count);
        }
        self.responses.extend(other.responses);
    }
}

#[derive(Serialize, Clone)]
pub struct Source {
    pub parsed: bool,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parsed: {}", self.parsed)
    }
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
    name: Option<String>,
    id: Option<String>,
    scope: Option<String>,
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
            name: None,
            id: None,
            scope: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_parsed_status() {
        let metadata = DiagnosticMetadata {
            id: "test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            uuid: "test".to_string(),
        };
        let mut report = DiagnosticReport::try_from(
            DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()),
        )
        .unwrap();

        let mut lookup = Lookup::<String>::new();
        lookup = lookup.was_parsed();
        lookup.add("data".to_string());

        report.add_lookup("my_lookup", &lookup);

        let stats = &report.diagnostic.lookup;
        assert!(stats.stats.contains_key("my_lookup"));
        assert!(stats.stats.get("my_lookup").unwrap().parsed);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_lookup_failure_recording() {
        let metadata = DiagnosticMetadata {
            id: "test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            uuid: "test".to_string(),
        };
        let mut report = DiagnosticReport::try_from(
            DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()),
        )
        .unwrap();

        let lookup = Lookup::<String>::new(); // parsed is false by default

        report.add_lookup("failed_lookup", &lookup);
        report.add_lookup("failed_lookup", &lookup); // Deduplication check

        let stats = &report.diagnostic.lookup;
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.failures.len(), 1);
        assert!(stats.failures.contains(&"failed_lookup".to_string()));
        assert!(!stats.stats.get("failed_lookup").unwrap().parsed);
    }
}
