// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::elasticsearch::{ClusterMetadata, License as ElasticsearchLicense};
use super::{DiagnosticManifest, DiagnosticMetadata, Lookup};
use crate::data::{Application, Platform};
use eyre::{OptionExt, Report, Result, eyre};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{NoneAsEmptyString, serde_as, skip_serializing_none};
use std::collections::HashMap;
use std::str::FromStr;

pub struct DiagnosticReportBuilder {
    cluster: Option<ClusterMetadata>,
    processors: HashMap<String, ProcessorSummary>,
    application: Option<Application>,
    metadata: DiagnosticMetadata,
    origin: Option<Origin>,
    /// Collection-stage events carried in from the manifest's per-request
    /// record — collection failures persist in the report, not only in logs.
    collection_events: Vec<DiagnosticEvent>,
}

impl DiagnosticReportBuilder {
    pub fn build(self) -> Result<DiagnosticReport> {
        DiagnosticReport::try_from(self)
    }

    pub fn application(self, application: Application) -> Self {
        Self {
            application: Some(application),
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
            application: None,
            origin: None,
            collection_events: Vec::new(),
        }
    }
}

impl TryFrom<DiagnosticManifest> for DiagnosticReportBuilder {
    type Error = eyre::Report;

    fn try_from(manifest: DiagnosticManifest) -> Result<Self> {
        let application = manifest.application();
        // Collection failures recorded at collect time persist as report
        // events (ADR-0016): a non-2xx request is a warning with its source.
        let collection_events = manifest
            .requested_apis
            .as_ref()
            .map(|apis| {
                apis.iter()
                    .filter(|(_, api)| !api.status.is_some_and(|status| (200..300).contains(&status)))
                    .map(|(name, api)| {
                        let reason = match api.status {
                            Some(status) => format!("collection failed with HTTP {status}"),
                            None => "collection failed without an HTTP response".to_string(),
                        };
                        DiagnosticEvent::warning(name.clone(), reason)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let metadata = DiagnosticMetadata::try_from(manifest)?;
        Ok(Self {
            cluster: None,
            metadata,
            processors: HashMap::new(),
            application,
            origin: None,
            collection_events,
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
    /// Deployment platform (replaces the legacy untyped `orchestration`
    /// identifier; legacy keys and identifier strings still deserialize)
    #[serde(alias = "orchestration", deserialize_with = "deserialize_platform_identifier")]
    pub platform: Option<Platform>,
}

/// Tolerant deserializer for the platform identifier: accepts the typed
/// platform values and the legacy `orchestration` strings; empty or
/// unrecognized values become `None` rather than failing the whole document.
fn deserialize_platform_identifier<'de, D>(deserializer: D) -> Result<Option<Platform>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    Ok(value.and_then(|value| Platform::from_str(value.trim()).ok()))
}
impl Identifiers {
    pub fn is_empty(&self) -> bool {
        self.account.is_none()
            && self.case_number.is_none()
            && self.filename.is_none()
            && self.opportunity.is_none()
            && self.user.is_none()
            && self.parent_id.is_none()
            && self.platform.is_none()
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
            platform: None,
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

    pub fn with_platform(self, platform: Platform) -> Self {
        Self {
            platform: Some(platform),
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
            platform: None,
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

/// The verdict of a diagnostic — one type for any diagnostic, parent or
/// child (ADR-0016). Derived from the report's recorded events, never set
/// imperatively; `Skipped` is constructed only where a diagnostic is
/// rejected before a report exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticOutcome {
    /// Everything selected was captured and processed.
    Complete,
    /// The common real case: some sources captured or exported, some failed.
    Partial,
    /// Nothing was produced.
    Failed,
    /// The diagnostic was not processed at all.
    Skipped(SkipKind),
}

/// Why a diagnostic was skipped (ADR-0019): deliberately out of scope, or
/// simply not implemented yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipKind {
    ByDesign,
    NotImplemented,
}

impl DiagnosticOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Skipped(_) => "skipped",
        }
    }

    pub fn skip_kind(&self) -> Option<SkipKind> {
        match self {
            Self::Skipped(kind) => Some(*kind),
            _ => None,
        }
    }
}

impl std::fmt::Display for DiagnosticOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Skipped(SkipKind::ByDesign) => write!(f, "skipped (by design)"),
            Self::Skipped(SkipKind::NotImplemented) => write!(f, "skipped (not implemented)"),
            other => write!(f, "{}", other.as_str()),
        }
    }
}

impl Serialize for DiagnosticOutcome {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Severity of a recorded diagnostic event.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    Error,
    Warning,
    Success,
}

/// One recorded event in a diagnostic's report: what happened, to which
/// source, and why. Failures are collected here, never dropped to logs
/// (ADR-0016). Events are source-grained (one per data source / processor /
/// exporter batch class), not per document.
#[derive(Serialize, Clone, Debug)]
pub struct DiagnosticEvent {
    pub severity: EventSeverity,
    /// The data source / processor / exporter the event pertains to.
    pub source: String,
    pub reason: String,
}

impl DiagnosticEvent {
    pub fn error(source: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            severity: EventSeverity::Error,
            source: source.into(),
            reason: reason.into(),
        }
    }

    pub fn warning(source: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            severity: EventSeverity::Warning,
            source: source.into(),
            reason: reason.into(),
        }
    }

    pub fn success(source: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            severity: EventSeverity::Success,
            source: source.into(),
            reason: reason.into(),
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
    /// Application component the diagnostic pertains to; absent for
    /// platform-only diagnostics (the platform rides on `identifiers`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<Application>,
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
    /// All recorded error/warning/success events (source + reason) — the
    /// persisted record failures land in, never only a log line (ADR-0016).
    pub events: Vec<DiagnosticEvent>,
    /// The derived verdict; kept equal to `derive_outcome(&events, &docs)` by
    /// construction (recomputed whenever events/doc counts change).
    pub outcome: DiagnosticOutcome,
}

/// Derive the diagnostic outcome from the recorded events and document
/// counts (ADR-0016): this is the only way an outcome is computed. Any error
/// with nothing produced is `Failed`; any error/warning or rejected document
/// alongside produced output is `Partial`; otherwise `Complete`. (`Skipped`
/// is constructed only where a diagnostic is rejected before a report
/// exists — a skip records no report at all.)
pub fn derive_outcome(events: &[DiagnosticEvent], docs: &Docs) -> DiagnosticOutcome {
    let has_error = events.iter().any(|event| event.severity == EventSeverity::Error);
    let has_warning = events.iter().any(|event| event.severity == EventSeverity::Warning);
    let has_failure_signal = has_error || has_warning || docs.errors > 0;
    let produced = docs.total > 0 || events.iter().any(|event| event.severity == EventSeverity::Success);

    if has_failure_signal && !produced {
        DiagnosticOutcome::Failed
    } else if has_failure_signal {
        DiagnosticOutcome::Partial
    } else {
        DiagnosticOutcome::Complete
    }
}

impl DiagnosticStats {
    /// The deployment platform recorded for this diagnostic. Total by
    /// construction: unresolved provenance is `Unknown`.
    pub fn platform(&self) -> Platform {
        self.identifiers.platform.unwrap_or_default()
    }

    fn refresh_outcome(&mut self) {
        self.outcome = derive_outcome(&self.events, &self.docs);
    }

    /// Display label per ADR-0001: the application when present, else the
    /// platform.
    pub fn display_label(&self) -> String {
        match self.application {
            Some(application) => application.to_string(),
            None => self.platform().to_string(),
        }
    }
}

impl DiagnosticReport {
    pub fn add_kibana_link(&mut self, link: String) {
        self.diagnostic.kibana_link = Some(link);
    }

    /// Record an event (source + reason) in the persisted report and keep the
    /// derived outcome in sync.
    pub fn record_event(&mut self, event: DiagnosticEvent) {
        self.diagnostic.events.push(event);
        self.diagnostic.refresh_outcome();
    }

    /// The derived verdict of this diagnostic.
    pub fn outcome(&self) -> DiagnosticOutcome {
        self.diagnostic.outcome
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

    fn add_single_processor_summary(&mut self, mut summary: ProcessorSummary) {
        // Drain the summary's recorded events into the persisted report and
        // record this source's verdict as an event (ADR-0016).
        let has_recorded_error = summary
            .events
            .iter()
            .any(|event| event.severity == EventSeverity::Error);
        self.diagnostic.events.append(&mut summary.events);
        if summary.source.missing {
            // Source was not present in an imported bundle; that is not a
            // processing failure and should not affect the derived outcome.
        } else if !summary.source.parsed {
            self.diagnostic.processor.errors += 1;
            self.diagnostic.processor.failures.push(summary.index.clone());
            if !has_recorded_error {
                self.diagnostic.events.push(DiagnosticEvent::error(
                    summary.processor.clone(),
                    "source could not be read or parsed".to_string(),
                ));
            }
        } else if summary.doc_errors > 0 {
            self.diagnostic.events.push(DiagnosticEvent::warning(
                summary.processor.clone(),
                format!(
                    "{} of {} documents rejected",
                    summary.doc_errors,
                    summary.docs + summary.doc_errors
                ),
            ));
        } else {
            self.diagnostic.events.push(DiagnosticEvent::success(
                summary.processor.clone(),
                format!("{} documents exported", summary.docs),
            ));
        }
        self.diagnostic.docs.created += summary.docs;
        self.diagnostic.docs.errors += summary.doc_errors;
        self.diagnostic.docs.total += summary.docs + summary.doc_errors;
        self.diagnostic.processor.push(summary.processor.clone(), summary);
        self.diagnostic.refresh_outcome();
    }

    pub fn add_lookup<T>(&mut self, name: &str, lookup: &Lookup<T>)
    where
        T: Clone + Serialize,
    {
        if lookup.missing {
            // Optional lookup source was absent from an imported bundle; this
            // should not affect the derived diagnostic outcome.
        } else if !lookup.parsed && !self.diagnostic.lookup.failures.iter().any(|f| f == name) {
            self.diagnostic.lookup.errors += 1;
            self.diagnostic.lookup.failures.push(name.to_string());
            self.diagnostic
                .events
                .push(DiagnosticEvent::warning(name.to_string(), "lookup could not be parsed"));
            self.diagnostic.refresh_outcome();
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
                application: builder.application,
                kibana_link: None,
                identifiers: Identifiers::default(),
                processing_duration: 0,
                outcome: derive_outcome(
                    &builder.collection_events,
                    &Docs {
                        created: 0,
                        errors: 0,
                        total: 0,
                    },
                ),
                events: builder.collection_events,
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

    /// Request status recorded when an HTTP exporter's request never
    /// completed (connection failure, serialization error). Status `0` is
    /// reserved exclusively for non-HTTP exporters (ADR-0016), so HTTP
    /// failures without a response use this sentinel instead.
    pub const HTTP_REQUEST_NOT_COMPLETED: u16 = 599;

    pub fn failed(failed_doc_count: u32, status_code: u16) -> Self {
        Self {
            batch_count: 1,
            status_counts: HashMap::new(),
            docs: 0,
            errors: failed_doc_count,
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
        // The scalar code is the transport verdict; `status_counts` is
        // authoritative for document outcomes. Mixed HTTP request codes are
        // never collapsed to 0 — 0 is reserved for non-HTTP exporters
        // (ADR-0016). A mixed aggregate keeps the most severe request code.
        self.status_code = match (self.status_code, other.status_code) {
            (0, status) if was_empty => status,
            (status, 0) if other.errors == 0 => status,
            (left, right) if left == right => left,
            (left, right) => match (left >= 400, right >= 400) {
                (true, true) => left.max(right),
                (true, false) => left,
                (false, true) => right,
                (false, false) => left.max(right),
            },
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
    /// Events recorded while producing this summary; drained into the
    /// diagnostic report's event log (ADR-0016).
    #[serde(skip_serializing)]
    events: Vec<DiagnosticEvent>,
}

impl ProcessorSummary {
    pub fn merge(&mut self, other: Result<ProcessorSummary>) {
        match other {
            Ok(other) => {
                self.batch.merge(other.batch);
                self.docs = self.docs.saturating_add(other.docs);
                self.doc_errors = self.doc_errors.saturating_add(other.doc_errors);
                self.events.extend(other.events);
            }
            Err(err) => {
                // A whole-source failure is a recorded event, never only a
                // log line (ADR-0016).
                self.events
                    .push(DiagnosticEvent::error(self.processor.clone(), err.to_string()));
            }
        }
    }

    pub fn add_child(&mut self, other: Result<ProcessorSummary>) {
        match other {
            Ok(other) => self.children.push(other),
            Err(err) => {
                self.events
                    .push(DiagnosticEvent::error(self.processor.clone(), err.to_string()));
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
    #[serde(skip_serializing_if = "is_false")]
    pub missing: bool,
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

fn is_false(value: &bool) -> bool {
    !*value
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
            source: Source {
                parsed: false,
                missing: false,
            },
            children: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn missing(name: String) -> Self {
        Self {
            source: Source {
                parsed: false,
                missing: true,
            },
            ..Self::new(name)
        }
    }

    pub fn with_error(mut self, reason: impl Into<String>) -> Self {
        self.events
            .push(DiagnosticEvent::error(self.processor.clone(), reason.into()));
        self
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
        self.source = Source {
            parsed: true,
            missing: false,
        };
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
    fn outcome_derives_complete_from_all_success_events() {
        let events = vec![DiagnosticEvent::success("nodes", "5 documents exported")];
        let docs = Docs {
            created: 5,
            errors: 0,
            total: 5,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Complete);
    }

    #[test]
    fn outcome_derives_partial_from_a_failure_alongside_output() {
        let events = vec![
            DiagnosticEvent::success("nodes", "5 documents exported"),
            DiagnosticEvent::error("tasks", "source could not be read or parsed"),
        ];
        let docs = Docs {
            created: 5,
            errors: 0,
            total: 5,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Partial);
    }

    #[test]
    fn outcome_derives_partial_from_rejected_documents() {
        // A 200 request with per-doc rejections is Partial: the per-doc
        // histogram is authoritative, not the transport code (ADR-0016)
        let events = vec![DiagnosticEvent::warning("nodes", "2 of 7 documents rejected")];
        let docs = Docs {
            created: 5,
            errors: 2,
            total: 7,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Partial);
    }

    #[test]
    fn outcome_derives_partial_when_all_attempted_documents_are_rejected() {
        let events = vec![DiagnosticEvent::warning("nodes", "7 of 7 documents rejected")];
        let docs = Docs {
            created: 0,
            errors: 7,
            total: 7,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Partial);
    }

    #[test]
    fn outcome_derives_failed_from_total_failure() {
        let events = vec![
            DiagnosticEvent::error("nodes", "connection refused"),
            DiagnosticEvent::error("tasks", "connection refused"),
        ];
        let docs = Docs {
            created: 0,
            errors: 0,
            total: 0,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Failed);
    }

    #[test]
    fn outcome_derives_failed_from_warning_without_output() {
        let events = vec![DiagnosticEvent::warning(
            "nodes",
            "collection failed without an HTTP response",
        )];
        let docs = Docs {
            created: 0,
            errors: 0,
            total: 0,
        };
        assert_eq!(derive_outcome(&events, &docs), DiagnosticOutcome::Failed);
    }

    #[test]
    fn skipped_outcome_distinguishes_by_design_from_not_implemented() {
        let by_design = DiagnosticOutcome::Skipped(SkipKind::ByDesign);
        let wip = DiagnosticOutcome::Skipped(SkipKind::NotImplemented);
        assert_eq!(by_design.as_str(), "skipped");
        assert_eq!(by_design.to_string(), "skipped (by design)");
        assert_eq!(wip.to_string(), "skipped (not implemented)");
        assert_eq!(by_design.skip_kind(), Some(SkipKind::ByDesign));
        assert_eq!(serde_json::to_value(by_design).unwrap(), "skipped (by design)");
        assert_eq!(serde_json::to_value(wip).unwrap(), "skipped (not implemented)");
    }

    #[test]
    fn merged_err_records_a_failure_event_not_a_dropped_log() {
        let mut summary = ProcessorSummary::new("metrics-nodes-esdiag".to_string());
        summary.merge(Err(eyre!("boom")));
        summary.add_child(Err(eyre!("child boom")));

        assert_eq!(summary.events.len(), 2);
        assert!(summary.events.iter().all(|e| e.severity == EventSeverity::Error));
        assert!(summary.events.iter().any(|e| e.reason.contains("boom")));
    }

    #[test]
    fn missing_source_summary_does_not_change_outcome() {
        let metadata = DiagnosticMetadata {
            id: "test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            uuid: "test".to_string(),
        };
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()))
                .unwrap();

        let mut parsed = ProcessorSummary::new("metrics-nodes-esdiag".to_string());
        let mut batch = BatchResponse::new(2);
        batch.status_code = 200;
        parsed.add_batch(batch);
        report.add_processor_summary(parsed.was_parsed());
        report.add_processor_summary(ProcessorSummary::missing("metrics-tasks-esdiag".to_string()));

        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
        assert_eq!(report.diagnostic.processor.errors(), 0);
        assert_eq!(report.diagnostic.events.len(), 1);
        assert!(
            report
                .diagnostic
                .processor
                .stats()
                .get("metrics-tasks-esdiag")
                .expect("missing source summary")
                .source
                .missing
        );
    }

    #[test]
    fn failed_summary_with_recorded_event_does_not_add_generic_duplicate() {
        let metadata = DiagnosticMetadata {
            id: "test".to_string(),
            collection_date: 0,
            runner: "test".to_string(),
            uuid: "test".to_string(),
        };
        let mut report =
            DiagnosticReport::try_from(DiagnosticReportBuilder::from(metadata).receiver("file path".to_string()))
                .unwrap();

        report.add_processor_summary(
            ProcessorSummary::new("cluster_settings-esdiag".to_string())
                .with_error("Failed to read cluster_settings: corrupt payload"),
        );

        assert_eq!(report.outcome(), DiagnosticOutcome::Failed);
        assert_eq!(report.diagnostic.processor.errors(), 1);
        assert_eq!(report.diagnostic.events.len(), 1);
        assert_eq!(
            report.diagnostic.events[0].reason,
            "Failed to read cluster_settings: corrupt payload"
        );
    }

    #[test]
    fn mixed_http_request_codes_are_not_collapsed_to_zero() {
        let mut aggregate = BatchResponse::aggregate();
        let mut ok = BatchResponse::new(5);
        ok.status_code = 200;
        let mut rejected = BatchResponse::failed(2, 429);
        rejected.status_code = 429;
        aggregate.merge(ok);
        aggregate.merge(rejected);

        // The most severe request code wins; 0 is reserved for non-HTTP
        // exporters (ADR-0016)
        assert_eq!(aggregate.status_code, 429);
        assert!(aggregate.status_counts.contains_key(&200));
        assert!(aggregate.status_counts.contains_key(&429));
    }

    #[test]
    fn non_http_exporter_status_stays_zero() {
        let mut aggregate = BatchResponse::aggregate();
        let mut local_a = BatchResponse::new(5);
        local_a.status_code = 0;
        let mut local_b = BatchResponse::new(3);
        local_b.status_code = 0;
        aggregate.merge(local_a);
        aggregate.merge(local_b);
        assert_eq!(aggregate.status_code, 0);
    }

    #[test]
    fn identifiers_platform_deserializes_from_legacy_orchestration_key() {
        let identifiers: Identifiers =
            serde_json::from_str(r#"{"user": "ada", "orchestration": "elastic-cloud-kubernetes"}"#)
                .expect("deserialize identifiers");
        assert_eq!(identifiers.platform, Some(crate::data::Platform::ECK));
    }

    #[test]
    fn identifiers_platform_tolerates_empty_and_unknown_legacy_values() {
        let empty: Identifiers = serde_json::from_str(r#"{"orchestration": ""}"#).expect("deserialize identifiers");
        assert_eq!(empty.platform, None);

        let unknown: Identifiers =
            serde_json::from_str(r#"{"orchestration": "not-a-platform"}"#).expect("deserialize identifiers");
        assert_eq!(unknown.platform, None);
    }

    #[test]
    fn identifiers_platform_serializes_as_typed_platform_key() {
        let identifiers = Identifiers::default().with_platform(crate::data::Platform::ECK);
        let value = serde_json::to_value(&identifiers).expect("serialize identifiers");
        assert_eq!(value["platform"], "eck");
        assert!(value.get("orchestration").is_none());
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
    fn processor_summary_records_successful_local_batch_as_status_200() {
        let mut summary = ProcessorSummary::new("metrics-node-esdiag".to_string());
        let mut batch = BatchResponse::new(3);
        batch.status_code = 200;

        summary.add_batch(batch);

        let value = serde_json::to_value(summary).expect("summary json");
        assert_eq!(value["batch"]["status_codes"]["200"], 1);
        assert_eq!(value["batch"]["status_codes"].get("0"), None);
        assert_eq!(value["docs"], 3);
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
