// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::elasticsearch::{ClusterMetadata, License as ElasticsearchLicense};
use super::{DiagnosticManifest, DiagnosticMetadata, Lookup};
use crate::data::Product;
use eyre::{OptionExt, Report, Result, eyre};
use serde::{Deserialize, Serialize};
use serde_with::{NoneAsEmptyString, serde_as, skip_serializing_none};
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
#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Identifiers {
    /// Account identifier
    #[serde_as(as = "NoneAsEmptyString")]
    pub account: Option<String>,
    /// Case number associated with the diagnostic
    #[serde_as(as = "NoneAsEmptyString")]
    pub case_number: Option<String>,
    /// Filename of the diagnostic bundle
    #[serde_as(as = "NoneAsEmptyString")]
    pub filename: Option<String>,
    /// Opportunity identifier
    #[serde_as(as = "NoneAsEmptyString")]
    pub opportunity: Option<String>,
    /// User who generated the diagnostic
    #[serde_as(as = "NoneAsEmptyString")]
    pub user: Option<String>,
    /// Parent diagnostic identifier
    #[serde_as(as = "NoneAsEmptyString")]
    pub parent_id: Option<String>,
    /// Orchestration platform
    #[serde_as(as = "NoneAsEmptyString")]
    pub orchestration: Option<String>,
}
impl Identifiers {
    pub fn is_empty(&self) -> bool {
        self.account.is_none()
            && self.case_number.is_none()
            && self.filename.is_none()
            && self.opportunity.is_none()
            && self.user.is_none()
            && self.parent_id.is_none()
            && self.orchestration.is_none()
    }

    pub fn new(
        account: Option<String>,
        case_number: Option<String>,
        filename: Option<String>,
        opportunity: Option<String>,
        user: Option<String>,
    ) -> Self {
        Self {
            account: normalize_identifier(account),
            case_number: normalize_identifier(case_number),
            filename: normalize_identifier(filename),
            opportunity: normalize_identifier(opportunity),
            user: normalize_identifier(user).or_else(|| normalize_identifier(std::env::var("ESDIAG_USER").ok())),
            parent_id: None,
            orchestration: None,
        }
    }

    pub fn default_user(self, username: Option<&String>) -> Self {
        Self {
            user: self.user.or_else(|| normalize_identifier(username.cloned())),
            ..self
        }
    }

    pub fn filename_as_str(&self) -> &str {
        self.filename.as_deref().unwrap_or("none")
    }

    pub fn with_filename(self, filename: Option<String>) -> Self {
        Self {
            filename: normalize_identifier(filename),
            ..self
        }
    }

    pub fn with_parent_id(self, parent_id: String) -> Self {
        Self {
            parent_id: normalize_identifier(Some(parent_id)),
            ..self
        }
    }

    pub fn with_orchestration(self, orchestration: String) -> Self {
        Self {
            orchestration: normalize_identifier(Some(orchestration)),
            ..self
        }
    }
}

impl Default for Identifiers {
    fn default() -> Self {
        let user = normalize_identifier(std::env::var("ESDIAG_USER").ok());
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

fn normalize_identifier(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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

#[cfg(test)]
impl NestedStats<ProcessorSummary> {
    fn stats(&self) -> &HashMap<String, ProcessorSummary> {
        &self.stats
    }

    fn errors(&self) -> u32 {
        self.errors
    }

    fn count(&self) -> u32 {
        self.count
    }
}

impl DiagnosticReport {
    pub fn add_processor_summary(&mut self, summary: ProcessorSummary) {
        for summary in summary.into_summaries() {
            self.add_single_processor_summary(summary);
        }
    }

    fn add_single_processor_summary(&mut self, summary: ProcessorSummary) {
        if !summary.source.parsed {
            self.diagnostic.processor.errors += 1;
            self.diagnostic.processor.failures.push(summary.index.clone());
        }
        self.diagnostic.docs.created += summary.docs;
        self.diagnostic.docs.errors += summary.doc_errors;
        self.diagnostic.docs.total += summary.docs + summary.doc_errors;
        self.diagnostic.processor.push(summary.processor.clone(), summary);
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

#[derive(Serialize, Clone)]
pub struct BatchResponse {
    #[serde(skip_serializing)]
    pub batch_count: u32,
    #[serde(skip_serializing)]
    status_counts: HashMap<u16, u32>,
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
            batch_count: 1,
            status_counts: HashMap::new(),
            docs,
            errors: 0,
            retries: 0,
            size: 0,
            status_code: 0,
            time: 0,
        }
    }

    pub fn aggregate() -> Self {
        Self {
            batch_count: 0,
            status_counts: HashMap::new(),
            docs: 0,
            errors: 0,
            retries: 0,
            size: 0,
            status_code: 0,
            time: 0,
        }
    }

    pub fn failed(error_count: u32, status_code: u16) -> Self {
        Self {
            batch_count: 1,
            status_counts: HashMap::new(),
            docs: 0,
            errors: error_count,
            retries: 0,
            size: 0,
            status_code,
            time: 0,
        }
    }

    pub fn merge(&mut self, other: Self) {
        let was_empty = self.batch_count == 0;
        self.batch_count = self.batch_count.saturating_add(other.batch_count);
        self.docs = self.docs.saturating_add(other.docs);
        self.errors = self.errors.saturating_add(other.errors);
        self.retries = self.retries.saturating_add(other.retries);
        self.size = self.size.saturating_add(other.size);
        self.time = self.time.saturating_add(other.time);
        self.status_code = match (self.status_code, other.status_code) {
            (0, status) if was_empty => status,
            (status, 0) if other.errors == 0 => status,
            (left, right) if left == right => left,
            _ => 0,
        };
        self.merge_status_counts(&other);
    }

    fn merge_status_counts(&mut self, other: &Self) {
        if other.status_counts.is_empty() {
            if other.batch_count > 0 {
                self.status_counts
                    .entry(other.status_code)
                    .and_modify(|count| *count = count.saturating_add(other.batch_count))
                    .or_insert(other.batch_count);
            }
            return;
        }

        for (status, count) in &other.status_counts {
            self.status_counts
                .entry(*status)
                .and_modify(|existing| *existing = existing.saturating_add(*count))
                .or_insert(*count);
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
    #[serde(skip_serializing)]
    children: Vec<ProcessorSummary>,
}

impl ProcessorSummary {
    pub fn merge(&mut self, other: Result<ProcessorSummary>) {
        match other {
            Ok(other) => {
                self.batch.merge(other.batch);
                self.docs = self.docs.saturating_add(other.docs);
                self.doc_errors = self.doc_errors.saturating_add(other.doc_errors);
            }
            Err(err) => {
                tracing::warn!("processor summary was err: {}", err);
            }
        }
    }

    pub fn add_child(&mut self, other: Result<ProcessorSummary>) {
        match other {
            Ok(other) => self.children.push(other),
            Err(err) => {
                tracing::warn!("processor summary was err: {}", err);
            }
        }
    }

    fn into_summaries(mut self) -> Vec<ProcessorSummary> {
        let children = std::mem::take(&mut self.children);
        let mut summaries = Vec::with_capacity(children.len() + 1);
        summaries.push(self);
        for child in children {
            summaries.extend(child.into_summaries());
        }
        summaries
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
        self.count = self.count.saturating_add(other.count);
        self.retries = self.retries.saturating_add(other.retries);
        for (code, count) in other.status_codes {
            self.status_codes
                .entry(code)
                .and_modify(|c| *c = c.saturating_add(count))
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

impl Source {
    #[cfg(test)]
    fn parsed(&self) -> bool {
        self.parsed
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
            children: Vec::new(),
        }
    }

    pub fn add_batch(&mut self, batch: BatchResponse) {
        if batch.batch_count == 0 && batch.docs == 0 && batch.errors == 0 {
            return;
        }

        self.batch.count = self.batch.count.saturating_add(batch.batch_count);
        self.batch.retries = self.batch.retries.saturating_add(batch.retries);
        if batch.status_counts.is_empty() {
            self.batch
                .status_codes
                .entry(batch.status_code)
                .and_modify(|count| *count = count.saturating_add(batch.batch_count))
                .or_insert(batch.batch_count);
        } else {
            for (status, count) in &batch.status_counts {
                self.batch
                    .status_codes
                    .entry(*status)
                    .and_modify(|existing| *existing = existing.saturating_add(*count))
                    .or_insert(*count);
            }
        }
        self.docs = self.docs.saturating_add(batch.docs);
        self.doc_errors = self.doc_errors.saturating_add(batch.errors);
        self.batch.responses.push(batch);
    }

    pub fn was_parsed(mut self) -> Self {
        self.source = Source { parsed: true };
        self.children = self.children.into_iter().map(ProcessorSummary::was_parsed).collect();
        self
    }

    pub fn rename(self, name: String) -> Self {
        Self {
            processor: name,
            ..self
        }
    }

    #[cfg(test)]
    fn doc_errors(&self) -> u32 {
        self.doc_errors
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
        let (r#type, path) = receiver.split_once(' ').ok_or_eyre("Invalid receiver string")?;
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
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()))
                .unwrap();

        let mut lookup = Lookup::<String>::new();
        lookup.parsed = true;
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
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()))
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

    #[test]
    fn identifiers_deserialize_empty_strings_as_none() {
        let identifiers: Identifiers = serde_yaml::from_str(
            r#"
account: ''
case_number: ''
user: ada
"#,
        )
        .expect("deserialize identifiers");

        assert_eq!(identifiers.account, None);
        assert_eq!(identifiers.case_number, None);
        assert_eq!(identifiers.user.as_deref(), Some("ada"));
    }

    #[test]
    fn identifiers_do_not_serialize_empty_values() {
        let identifiers = Identifiers::new(
            Some("".to_string()),
            Some("  ".to_string()),
            None,
            None,
            Some("ada".to_string()),
        );

        let yaml = serde_yaml::to_string(&identifiers).expect("serialize identifiers");

        assert!(!yaml.contains("account"));
        assert!(!yaml.contains("case_number"));
        assert!(yaml.contains("user: ada"));
    }

    #[test]
    fn processor_summary_preserves_aggregated_batch_status_counts() {
        let mut aggregate = BatchResponse::aggregate();
        let mut first = BatchResponse::new(2);
        first.status_code = 200;
        let mut second = BatchResponse::new(3);
        second.status_code = 200;
        aggregate.merge(first);
        aggregate.merge(second);

        let mut summary = ProcessorSummary::new("metrics-task-esdiag".to_string());
        summary.add_batch(aggregate);

        let value = serde_json::to_value(summary).expect("summary json");
        assert_eq!(value["batch"]["count"], 2);
        assert_eq!(value["batch"]["status_codes"]["200"], 2);
        assert_eq!(value["docs"], 5);
    }

    #[test]
    fn batch_response_merge_saturates_summary_counters() {
        let mut left = BatchResponse {
            batch_count: u32::MAX,
            status_counts: std::collections::HashMap::from([(200, u32::MAX)]),
            docs: u32::MAX,
            errors: u32::MAX,
            retries: u16::MAX,
            size: u32::MAX,
            status_code: 200,
            time: u32::MAX,
        };
        let mut right = BatchResponse::new(1);
        right.errors = 1;
        right.retries = 1;
        right.size = 1;
        right.status_code = 200;
        right.time = 1;
        right.status_counts.insert(200, 1);

        left.merge(right);

        assert_eq!(left.batch_count, u32::MAX);
        assert_eq!(left.docs, u32::MAX);
        assert_eq!(left.errors, u32::MAX);
        assert_eq!(left.retries, u16::MAX);
        assert_eq!(left.size, u32::MAX);
        assert_eq!(left.time, u32::MAX);
        assert_eq!(left.status_counts[&200], u32::MAX);
    }

    #[test]
    fn processor_summary_add_batch_saturates_summary_counters() {
        let mut summary = ProcessorSummary::new("metrics-task-esdiag".to_string());
        summary.batch.count = u32::MAX;
        summary.batch.retries = u16::MAX;
        summary.batch.status_codes.insert(200, u32::MAX);
        summary.docs = u32::MAX;
        summary.doc_errors = u32::MAX;

        let mut batch = BatchResponse::new(1);
        batch.errors = 1;
        batch.retries = 1;
        batch.status_code = 200;

        summary.add_batch(batch);

        assert_eq!(summary.batch.count, u32::MAX);
        assert_eq!(summary.batch.retries, u16::MAX);
        assert_eq!(summary.batch.status_codes[&200], u32::MAX);
        assert_eq!(summary.docs, u32::MAX);
        assert_eq!(summary.doc_errors, u32::MAX);
    }

    #[test]
    fn processor_summary_ignores_empty_noop_batch_response() {
        let mut empty = BatchResponse::aggregate();
        empty.status_code = 200;

        let mut summary = ProcessorSummary::new("metrics-ingest.pipeline-esdiag".to_string());
        summary.add_batch(empty);

        let value = serde_json::to_value(summary).expect("summary json");
        assert_eq!(value["batch"]["count"], 0);
        assert_eq!(value["batch"]["status_codes"], serde_json::json!({}));
        assert_eq!(value["docs"], 0);
    }

    #[test]
    fn diagnostic_report_flattens_child_processor_summaries() {
        let metadata = DiagnosticMetadata {
            id: "test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            uuid: "test".to_string(),
        };
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()))
                .unwrap();

        let mut parent = ProcessorSummary::new("metrics-node-esdiag".to_string());
        let mut parent_batch = BatchResponse::new(4);
        parent_batch.status_code = 200;
        parent.add_batch(parent_batch);

        let mut child = ProcessorSummary::new("metrics-node.http.clients-esdiag".to_string());
        let mut child_batch = BatchResponse::failed(2, 400);
        child_batch.status_code = 400;
        child.add_batch(child_batch);

        parent.add_child(Ok(child));
        report.add_processor_summary(parent.was_parsed());

        let stats = report.diagnostic.processor.stats();
        assert_eq!(report.diagnostic.processor.count(), 2);
        assert_eq!(report.diagnostic.processor.errors(), 0);
        assert_eq!(report.diagnostic.docs.created, 4);
        assert_eq!(report.diagnostic.docs.errors, 2);
        assert!(stats["metrics-node-esdiag"].source.parsed());
        assert!(stats["metrics-node.http.clients-esdiag"].source.parsed());
        assert_eq!(stats["metrics-node.http.clients-esdiag"].doc_errors(), 2);
    }
}
