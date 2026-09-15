// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kibana diagnostic processor
//!
//! This module provides the entry point for processing Kibana diagnostic bundles.
//! It follows a single-node workflow similar to Logstash, extracting metrics
//! and settings from various JSON files.

/// Kibana alerts processor
mod alerts;
/// Collector definition for Kibana diagnostics
mod collector;
/// Kibana detection engine processor
mod detection_engine;
/// Kibana fleet processor
mod fleet;
/// Kibana health processor
mod health;
/// Kibana diagnostic metadata
mod metadata;
/// Kibana node stats processor
mod node_stats;
/// Kibana security processor
mod security;
/// Kibana settings processor
mod settings;
/// Kibana spaces processor
mod spaces;
/// Kibana status processor
mod status;
/// Kibana synthetics and uptime processor
mod synthetics_uptime;
/// Kibana version
mod version;

pub use collector::KibanaCollector;

use super::{
    DiagnosticProcessor, DocumentExporter, Metadata, ProcessorSummary,
    api::ProcessSelection,
    diagnostic::{
        DataSource, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder,
        data_source::{ProcessableClaim, validate_processable_registry},
    },
};
use crate::{
    data::{self, Application},
    exporter::Exporter,
    receiver::{MissingSource, Receiver},
};
use alerts::{AlertHealth, Alerts};
use detection_engine::{DetectionEngineHealth, DetectionEngineRules};
use eyre::{Result, eyre};
use fleet::{AgentPolicies, AgentStatus, Agents, Packages};
use futures::future::BoxFuture;
use health::{StackMonitoringHealth, TaskManagerHealth};
use metadata::KibanaMetadata;
use node_stats::NodeStats;
use security::{Actions, Roles, Users};
use serde::{Serialize, de::DeserializeOwned};
use settings::{FleetSettings, UptimeSettings};
use spaces::Spaces;
use status::Status;
use std::{collections::HashSet, sync::Arc};
use synthetics_uptime::{SyntheticsFilters, UptimeLocations};
use tokio::sync::mpsc;

type KibanaProcessFn = for<'a> fn(&'a KibanaDiagnostic, mpsc::Sender<ProcessorSummary>) -> BoxFuture<'a, Result<()>>;

struct KibanaDispatchEntry {
    key: &'static str,
    datasource_name: fn() -> String,
    process: KibanaProcessFn,
}

macro_rules! processes {
    ($source:ty) => {{
        fn run<'a>(
            diagnostic: &'a KibanaDiagnostic,
            summary_tx: mpsc::Sender<ProcessorSummary>,
        ) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move { diagnostic.process_datasource::<$source>(summary_tx).await })
        }
        run
    }};
}

const KIBANA_DISPATCH: &[KibanaDispatchEntry] = &[
    KibanaDispatchEntry {
        key: "kibana_stats",
        datasource_name: datasource_name::<NodeStats>,
        process: processes!(NodeStats),
    },
    KibanaDispatchEntry {
        key: "kibana_status",
        datasource_name: datasource_name::<Status>,
        process: processes!(Status),
    },
    KibanaDispatchEntry {
        key: "kibana_fleet_settings",
        datasource_name: datasource_name::<FleetSettings>,
        process: processes!(FleetSettings),
    },
    KibanaDispatchEntry {
        key: "kibana_uptime_settings",
        datasource_name: datasource_name::<UptimeSettings>,
        process: processes!(UptimeSettings),
    },
    KibanaDispatchEntry {
        key: "kibana_roles",
        datasource_name: datasource_name::<Roles>,
        process: processes!(Roles),
    },
    KibanaDispatchEntry {
        key: "kibana_user",
        datasource_name: datasource_name::<Users>,
        process: processes!(Users),
    },
    KibanaDispatchEntry {
        key: "kibana_actions",
        datasource_name: datasource_name::<Actions>,
        process: processes!(Actions),
    },
    KibanaDispatchEntry {
        key: "kibana_fleet_agents",
        datasource_name: datasource_name::<Agents>,
        process: processes!(Agents),
    },
    KibanaDispatchEntry {
        key: "kibana_fleet_agent_policies",
        datasource_name: datasource_name::<AgentPolicies>,
        process: processes!(AgentPolicies),
    },
    KibanaDispatchEntry {
        key: "kibana_fleet_packages",
        datasource_name: datasource_name::<Packages>,
        process: processes!(Packages),
    },
    KibanaDispatchEntry {
        key: "kibana_fleet_agent_status",
        datasource_name: datasource_name::<AgentStatus>,
        process: processes!(AgentStatus),
    },
    KibanaDispatchEntry {
        key: "kibana_alerts",
        datasource_name: datasource_name::<Alerts>,
        process: processes!(Alerts),
    },
    KibanaDispatchEntry {
        key: "kibana_alerts_health",
        datasource_name: datasource_name::<AlertHealth>,
        process: processes!(AlertHealth),
    },
    KibanaDispatchEntry {
        key: "kibana_spaces",
        datasource_name: datasource_name::<Spaces>,
        process: processes!(Spaces),
    },
    KibanaDispatchEntry {
        key: "kibana_task_manager_health",
        datasource_name: datasource_name::<TaskManagerHealth>,
        process: processes!(TaskManagerHealth),
    },
    KibanaDispatchEntry {
        key: "kibana_stack_monitoring_health",
        datasource_name: datasource_name::<StackMonitoringHealth>,
        process: processes!(StackMonitoringHealth),
    },
    KibanaDispatchEntry {
        key: "kibana_synthetics_monitor_filters",
        datasource_name: datasource_name::<SyntheticsFilters>,
        process: processes!(SyntheticsFilters),
    },
    KibanaDispatchEntry {
        key: "kibana_uptime_locations",
        datasource_name: datasource_name::<UptimeLocations>,
        process: processes!(UptimeLocations),
    },
    KibanaDispatchEntry {
        key: "kibana_detection_engine_health_cluster",
        datasource_name: datasource_name::<DetectionEngineHealth>,
        process: processes!(DetectionEngineHealth),
    },
    KibanaDispatchEntry {
        key: "kibana_detection_engine_rules_installed",
        datasource_name: datasource_name::<DetectionEngineRules>,
        process: processes!(DetectionEngineRules),
    },
];

fn datasource_name<T: DataSource>() -> String {
    T::name()
}

fn validate_kibana_dispatch_registry() -> Result<()> {
    static VALIDATED: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();
    VALIDATED
        .get_or_init(|| {
            let claims = KIBANA_DISPATCH
                .iter()
                .map(|entry| ProcessableClaim {
                    key: entry.key,
                    datasource_name: (entry.datasource_name)(),
                })
                .collect::<Vec<_>>();
            validate_processable_registry("kibana", &claims).map_err(|error| error.to_string())
        })
        .clone()
        .map_err(|error| eyre!(error))
}

#[derive(Serialize)]
pub struct KibanaDiagnostic {
    lookups: Lookups,
    metadata: KibanaMetadata,
    selected_processors: Option<HashSet<String>>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

#[derive(Serialize)]
struct Lookups {}

impl KibanaDiagnostic {
    fn should_process(&self, key: &str) -> bool {
        self.selected_processors
            .as_ref()
            .is_none_or(|selected| selected.contains(key))
    }

    #[cfg(test)]
    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }

    async fn process_datasource<T>(&self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()>
    where
        T: DataSource + DocumentExporter<Lookups, KibanaMetadata> + DeserializeOwned + Send + Sync,
    {
        let instances = match self.receiver.get_all::<T>().await {
            Ok(instances) => instances,
            Err(error) if is_missing_source_error(&error) => {
                summary_tx
                    .send(ProcessorSummary::missing(T::name()))
                    .await
                    .map_err(|error| eyre!(error))?;
                return Ok(());
            }
            Err(error) => vec![Err(error)],
        };

        let mut summary: Option<ProcessorSummary> = None;
        let mut pending_errors = Vec::new();
        for instance in instances {
            match instance {
                Ok(data) => {
                    let next = data
                        .documents_export(&self.exporter, &self.lookups, &self.metadata)
                        .await;
                    match &mut summary {
                        Some(summary) => summary.merge(Ok(next)),
                        None => summary = Some(next),
                    }
                }
                Err(error) if is_missing_source_error(&error) => {}
                Err(error) => match &mut summary {
                    Some(summary) => summary.merge(Err(error)),
                    None => pending_errors.push(error),
                },
            }
        }

        let mut summary = match summary {
            Some(summary) => summary.was_parsed(),
            None if pending_errors.is_empty() => ProcessorSummary::missing(T::name()),
            None => ProcessorSummary::new(T::name()),
        };
        for error in pending_errors {
            summary.merge(Err(error));
        }
        summary_tx.send(summary).await.map_err(|error| {
            tracing::error!("Failed to send summary: {}", error);
            eyre!(error)
        })
    }
}

fn is_missing_source_error(error: &eyre::Report) -> bool {
    error.chain().any(|cause| {
        cause.is::<MissingSource>()
            || cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
}

async fn send_documents<T>(exporter: &Exporter, data_stream: &str, documents: Vec<T>) -> ProcessorSummary
where
    T: Serialize + Send + Sync,
{
    let mut summary = ProcessorSummary::new(data_stream.to_string());
    match exporter.send(data_stream.to_string(), documents).await {
        Ok(batch) => summary.add_batch(batch),
        Err(error) => summary.merge(Err(error)),
    }
    summary
}

impl DiagnosticProcessor for KibanaDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let kibana_version = receiver.get::<version::Version>().await?;
        let metadata = KibanaMetadata::try_new(manifest, kibana_version)?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .application(Application::Kibana)
            .receiver(receiver.to_string())
            .build()?;

        Ok((
            Box::new(Self {
                lookups: Lookups {},
                receiver,
                exporter,
                metadata,
                selected_processors: process_selection.map(|selection| selection.selected.into_iter().collect()),
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        tracing::debug!("Running Kibana diagnostic processors");
        if tracing::enabled!(tracing::Level::DEBUG) {
            data::save_file("diagnostic.json", &self)?;
        }

        validate_kibana_dispatch_registry()?;
        for entry in KIBANA_DISPATCH {
            if self.should_process(entry.key) {
                (entry.process)(&self, summary_tx.clone()).await?;
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            self.metadata.node.name.clone(),
            self.metadata.node.id.clone(),
            "node".to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::Uri,
        processor::{DiagnosticOutcome, api::ApiResolver},
    };
    use serde_json::{Value, json};
    use std::{
        fs::{self, File},
        io::Write,
        path::Path,
    };
    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    const STATUS: &str = r#"{
        "name": "kibana-node",
        "uuid": "kibana-node-id",
        "version": {
            "number": "8.19.3",
            "build_hash": "abc123",
            "build_number": 1,
            "build_snapshot": false
        },
        "status": {
            "overall": {
                "level": "available"
            },
            "plugins": {
                "taskManager": {
                    "level": "available"
                }
            }
        }
    }"#;

    async fn run_bundle(files: Vec<(&str, String)>, selected: Option<Vec<&str>>) -> (TempDir, DiagnosticReport) {
        let input = tempfile::tempdir().expect("input directory");
        fs::write(
            input.path().join(DiagnosticManifest::FILENAME),
            json!({
                "mode": "support",
                "product": "kibana",
                "diagnostic": "kibana-test",
                "type": "kibana-diagnostics",
                "runner": "esdiag",
                "version": "8.19.3",
                "timestamp": "2026-08-30T12:00:00Z",
                "collection_date_millis": 1788091200000_u64
            })
            .to_string(),
        )
        .expect("write manifest");
        fs::write(input.path().join("kibana_status.json"), STATUS).expect("write status");
        for (path, contents) in files {
            let path = input.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create source directory");
            }
            fs::write(path, contents).expect("write source");
        }

        let receiver = Arc::new(Receiver::try_from(Uri::Directory(input.path().to_path_buf())).expect("receiver"));
        run_receiver(receiver, selected).await
    }

    async fn run_receiver(receiver: Arc<Receiver>, selected: Option<Vec<&str>>) -> (TempDir, DiagnosticReport) {
        let manifest = receiver.try_get_manifest().await.expect("manifest");
        let output = tempfile::tempdir().expect("output directory");
        let exporter = Arc::new(Exporter::try_from(Uri::Directory(output.path().to_path_buf())).expect("exporter"));
        let selection = selected.map(|selected| ProcessSelection {
            product: "kibana".to_string(),
            diagnostic_type: "custom".to_string(),
            selected: selected.into_iter().map(str::to_string).collect(),
        });
        let (diagnostic, mut report) = KibanaDiagnostic::try_new(receiver, exporter, manifest, selection)
            .await
            .expect("Kibana diagnostic");
        let (summary_tx, mut summary_rx) = mpsc::channel(64);
        diagnostic.process(summary_tx).await.expect("process diagnostic");
        while let Some(summary) = summary_rx.recv().await {
            report.add_processor_summary(summary);
        }
        (output, report)
    }

    fn read_stream(output: &Path, stream: &str) -> Vec<Value> {
        let contents = fs::read_to_string(output.join(format!("{stream}.ndjson")))
            .unwrap_or_else(|error| panic!("read {stream}: {error}"));
        contents
            .lines()
            .map(|line| serde_json::from_str(line).expect("parse output document"))
            .collect()
    }

    fn assert_shared_metadata(document: &Value) {
        assert!(
            document["diagnostic"]["id"]
                .as_str()
                .is_some_and(|identifier| !identifier.is_empty())
        );
        assert_eq!(document["diagnostic"]["version"], "kibana-test");
        assert_eq!(document["@timestamp"], 1788091200000_u64);
        assert_eq!(document["node"]["name"], "kibana-node");
        assert_eq!(document["node"]["id"], "kibana-node-id");
        assert_eq!(document["node"]["version"]["number"], "8.19.3");
    }

    #[tokio::test]
    async fn core_sources_export_node_status_and_shared_metadata() {
        let (output, report) = run_bundle(
            vec![(
                "kibana_stats.json",
                json!({
                    "process": {"memory": {"heap": 42}},
                    "os": {"load": {"1m": 0.5}},
                    "response_times": {"average": 12}
                })
                .to_string(),
            )],
            None,
        )
        .await;

        let node = read_stream(output.path(), "metrics-kibana.node-esdiag");
        let status = read_stream(output.path(), "metrics-kibana.status-esdiag");
        assert_eq!(node.len(), 1);
        assert_eq!(status.len(), 1);
        assert_eq!(node[0]["node"]["process"]["memory"]["heap"], 42);
        assert_eq!(node[0]["node"]["os"]["load"]["1m"], 0.5);
        assert_eq!(node[0]["node"]["response_times"]["average"], 12);
        assert_eq!(status[0]["status"]["overall"]["level"], "available");
        assert_shared_metadata(&node[0]);
        assert_shared_metadata(&status[0]);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn settings_security_and_spaces_export_to_their_contract_streams() {
        let (output, report) = run_bundle(
            vec![
                ("kibana_fleet_settings.json", json!({"fleet_setting": true}).to_string()),
                (
                    "kibana_uptime_settings.json",
                    json!({"uptime_setting": true}).to_string(),
                ),
                ("kibana_roles.json", json!([{"role": "admin"}]).to_string()),
                ("kibana_user.json", json!({"username": "elastic"}).to_string()),
                (
                    "spaces/default/kibana_actions.json",
                    json!([{"action": "email"}]).to_string(),
                ),
                ("kibana_spaces.json", json!([{"id": "default"}]).to_string()),
            ],
            None,
        )
        .await;

        assert_eq!(read_stream(output.path(), "settings-kibana.fleet-esdiag").len(), 1);
        assert_eq!(read_stream(output.path(), "settings-kibana.uptime-esdiag").len(), 1);
        assert_eq!(read_stream(output.path(), "settings-kibana.security-esdiag").len(), 3);
        assert_eq!(read_stream(output.path(), "settings-kibana.spaces-esdiag").len(), 1);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn fleet_processes_every_page_and_legacy_numbered_file() {
        let (output, report) = run_bundle(
            vec![
                (
                    "pages/page-0001/kibana_fleet_agents.json",
                    json!({"list": [{"agent": 1}, {"agent": 2}]}).to_string(),
                ),
                (
                    "pages/page-0002/kibana_fleet_agents.json",
                    json!({"list": [{"agent": 3}]}).to_string(),
                ),
                (
                    "kibana_fleet_agent_policies_1.json",
                    json!({"items": [{"policy": 1}]}).to_string(),
                ),
                ("kibana_fleet_packages.json", json!([{"package": 1}]).to_string()),
                (
                    "kibana_fleet_agent_status.json",
                    json!({"online": 3, "offline": 0}).to_string(),
                ),
            ],
            None,
        )
        .await;

        assert_eq!(read_stream(output.path(), "settings-kibana.fleet-esdiag").len(), 5);
        assert_eq!(read_stream(output.path(), "metrics-kibana.fleet-esdiag").len(), 1);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn alerts_process_space_pages_legacy_files_and_health() {
        let (output, report) = run_bundle(
            vec![
                (
                    "spaces/default/pages/page-0001/kibana_alerts.json",
                    json!({"data": [{"alert": 1}]}).to_string(),
                ),
                (
                    "spaces/marketing/kibana_alerts_1.json",
                    json!({"data": [{"alert": 2}]}).to_string(),
                ),
                ("kibana_alerts_2.json", json!({"data": [{"alert": 3}]}).to_string()),
                ("kibana_alerts_health.json", json!({"status": "ok"}).to_string()),
            ],
            None,
        )
        .await;

        assert_eq!(read_stream(output.path(), "settings-kibana.alerts-esdiag").len(), 3);
        assert_eq!(read_stream(output.path(), "metrics-kibana.alerts-esdiag").len(), 1);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn health_sources_export_task_manager_and_stack_monitoring() {
        let (output, report) = run_bundle(
            vec![
                ("kibana_task_manager_health.json", json!({"status": "ok"}).to_string()),
                (
                    "kibana_stack_monitoring_health.json",
                    json!({"status": "available"}).to_string(),
                ),
            ],
            None,
        )
        .await;

        assert_eq!(
            read_stream(output.path(), "metrics-kibana.task_manager-esdiag").len(),
            1
        );
        assert_eq!(
            read_stream(output.path(), "metrics-kibana.stack_monitoring-esdiag").len(),
            1
        );
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn synthetics_and_uptime_sources_export_settings() {
        let (output, report) = run_bundle(
            vec![
                (
                    "kibana_synthetics_monitor_filters.json",
                    json!({"locations": ["us-east"]}).to_string(),
                ),
                (
                    "kibana_uptime_locations.json",
                    json!({"locations": [{"id": "us-east"}]}).to_string(),
                ),
            ],
            None,
        )
        .await;

        assert_eq!(read_stream(output.path(), "settings-kibana.synthetics-esdiag").len(), 1);
        assert_eq!(read_stream(output.path(), "settings-kibana.uptime-esdiag").len(), 1);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn detection_engine_processes_every_rule_page_and_health() {
        let (output, report) = run_bundle(
            vec![
                (
                    "pages/page-0001/kibana_detection_engine_rules_installed.json",
                    json!({"data": [{"rule": 1}]}).to_string(),
                ),
                (
                    "pages/page-0002/kibana_detection_engine_rules_installed.json",
                    json!({"data": [{"rule": 2}]}).to_string(),
                ),
                (
                    "kibana_detection_engine_health_cluster.json",
                    json!({"status": "healthy"}).to_string(),
                ),
            ],
            None,
        )
        .await;

        assert_eq!(
            read_stream(output.path(), "settings-kibana.detection_engine-esdiag").len(),
            2
        );
        assert_eq!(
            read_stream(output.path(), "metrics-kibana.detection_engine-esdiag").len(),
            1
        );
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn explicit_processing_selection_skips_unselected_sources() {
        let (output, report) = run_bundle(
            vec![
                (
                    "kibana_stats.json",
                    json!({"process": {}, "os": {}, "response_times": {}}).to_string(),
                ),
                ("kibana_alerts.json", json!({"data": [{"alert": 1}]}).to_string()),
            ],
            Some(vec!["kibana_status", "kibana_stats"]),
        )
        .await;

        assert!(output.path().join("metrics-kibana.status-esdiag.ndjson").exists());
        assert!(output.path().join("metrics-kibana.node-esdiag.ndjson").exists());
        assert!(!output.path().join("settings-kibana.alerts-esdiag.ndjson").exists());
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn malformed_present_source_is_recorded_as_a_processing_failure() {
        let (_output, report) = run_bundle(
            vec![("kibana_stats.json", "{not-json".to_string())],
            Some(vec!["kibana_status", "kibana_stats"]),
        )
        .await;

        assert_eq!(report.outcome(), DiagnosticOutcome::Partial);
        assert!(
            report
                .events()
                .iter()
                .any(|event| event.reason.contains("Failed to parse") && event.source == "kibana_stats")
        );
    }

    #[tokio::test]
    async fn archive_fixture_processes_space_scoped_sources() {
        let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archives/kibana-api-diagnostics-8.19.3.zip");
        let receiver = Arc::new(Receiver::try_from(Uri::File(archive)).expect("archive receiver"));
        receiver.try_get_manifest().await.expect("archive manifest");
        let alerts = receiver.get_all::<Alerts>().await.expect("scoped alerts");
        assert_eq!(alerts.len(), 1);
        assert!(alerts.into_iter().all(|alert| alert.is_ok()));
        let (output, report) = run_receiver(receiver, None).await;

        assert!(
            report
                .events()
                .iter()
                .any(|event| event.source == "settings-kibana.alerts-esdiag"),
            "scoped alerts were not processed"
        );
        assert!(
            report
                .events()
                .iter()
                .any(|event| event.source == "settings-kibana.security-esdiag"),
            "scoped actions were not processed"
        );
        assert!(output.path().join("metrics-kibana.status-esdiag.ndjson").exists());
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[tokio::test]
    async fn archive_processes_every_space_scoped_page() {
        let directory = tempfile::tempdir().expect("archive directory");
        let archive = directory.path().join("kibana-pages.zip");
        let mut writer = ZipWriter::new(File::create(&archive).expect("archive file"));
        let options = SimpleFileOptions::default();
        for (path, contents) in [
            (
                "bundle/diagnostic_manifest.json",
                json!({
                    "mode": "support",
                    "product": "kibana",
                    "diagnostic": "kibana-test",
                    "type": "kibana-diagnostics",
                    "runner": "esdiag",
                    "version": "8.19.3",
                    "timestamp": "2026-08-30T12:00:00Z",
                    "collection_date_millis": 1788091200000_u64
                })
                .to_string(),
            ),
            ("bundle/kibana_status.json", STATUS.to_string()),
            (
                "bundle/spaces/default/pages/page-0001/kibana_alerts.json",
                json!({"data": [{"alert": 1}]}).to_string(),
            ),
            (
                "bundle/spaces/default/pages/page-0002/kibana_alerts.json",
                json!({"data": [{"alert": 2}]}).to_string(),
            ),
        ] {
            writer.start_file(path, options).expect("archive entry");
            writer.write_all(contents.as_bytes()).expect("archive contents");
        }
        writer.finish().expect("finish archive");

        let receiver = Arc::new(Receiver::try_from(Uri::File(archive)).expect("archive receiver"));
        let (output, report) = run_receiver(receiver, Some(vec!["kibana_status", "kibana_alerts"])).await;

        assert_eq!(read_stream(output.path(), "settings-kibana.alerts-esdiag").len(), 2);
        assert_eq!(report.outcome(), DiagnosticOutcome::Complete);
    }

    #[test]
    fn registry_and_dispatch_table_agree() {
        validate_kibana_dispatch_registry().expect("Kibana registry");
    }

    #[test]
    fn processing_options_include_required_status_and_optional_sources() {
        let options = ApiResolver::resolve_processing_options("kibana", "standard", "").expect("processing options");
        let status = options
            .iter()
            .find(|option| option.key == "kibana_status")
            .expect("status option");
        let alerts = options
            .iter()
            .find(|option| option.key == "kibana_alerts")
            .expect("alerts option");

        assert!(status.required);
        assert!(!alerts.required);
        assert!(alerts.selected);
    }
}
