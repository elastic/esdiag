// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Agent identity from agent-info.yaml
mod agent_info;
/// Per-component beat metrics
mod beat_metrics;
/// Computed configuration
mod computed_config;
/// Per-component input metrics
mod input_metrics;
/// Agent local configuration
mod local_config;
/// Log file forwarding
mod logs;
/// Agent diagnostic metadata
pub mod metadata;
/// Per-component rendered configuration
mod rendered_config;
/// Agent runtime state
mod state;

pub use agent_info::AgentInfo;
pub use metadata::AgentMetadata;

use super::{
    DiagnosticProcessor, DocumentExporter, Metadata, ProcessorSummary,
    diagnostic::{DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder},
};
use crate::{
    data::Product,
    exporter::Exporter,
    receiver::Receiver,
};
use beat_metrics::BeatMetrics;
use computed_config::ComputedConfig;
use eyre::Result;
use input_metrics::InputMetrics;
use local_config::LocalConfig;
use rendered_config::RenderedConfig;
use serde::{Serialize, de::DeserializeOwned};
use state::State;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct AgentDiagnostic {
    lookups: Lookups,
    metadata: AgentMetadata,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
}

impl AgentDiagnostic {
    async fn process_datasource<T>(
        &mut self,
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()>
    where
        T: super::diagnostic::DataSource
            + DocumentExporter<Lookups, AgentMetadata>
            + DeserializeOwned
            + Send
            + Sync,
    {
        let data = match self.receiver.get::<T>().await {
            Ok(data) => data,
            Err(e) => {
                log::warn!("Optional source {} not available: {}", T::name(), e);
                return Ok(());
            }
        };
        let summary = data
            .documents_export(&self.exporter, &self.lookups, &self.metadata)
            .await;
        summary_tx.send(summary).await.map_err(|err| {
            log::error!("Failed to send summary: {}", err);
            eyre::eyre!(err)
        })
    }

    async fn process_components(
        &self,
        component_ids: &[String],
        summary_tx: mpsc::Sender<ProcessorSummary>,
    ) -> Result<()> {
        let mut all_beat_docs = Vec::new();
        let mut all_input_docs = Vec::new();
        let mut all_inputs = Vec::new();
        let mut all_outputs = Vec::new();
        let mut all_features = Vec::new();
        let mut all_apm = Vec::new();

        for component_id in component_ids {
            let component_dir = format!("components/{}", component_id);
            let component_receiver = match self.receiver.clone_for_subdir(&component_dir) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Cannot access component directory {}: {}", component_dir, e);
                    continue;
                }
            };

            // beat_metrics.json
            match component_receiver.get::<BeatMetrics>().await {
                Ok(bm) => all_beat_docs.push(bm.into_doc(component_id, &self.metadata)),
                Err(e) => log::debug!(
                    "No beat_metrics.json for component {}: {}",
                    component_id,
                    e
                ),
            }

            // input_metrics.json
            match component_receiver.get::<InputMetrics>().await {
                Ok(im) => all_input_docs.extend(im.into_docs(&self.metadata)),
                Err(e) => log::debug!(
                    "No input_metrics.json for component {}: {}",
                    component_id,
                    e
                ),
            }

            // beat-rendered-config.yml
            match component_receiver.get::<RenderedConfig>().await {
                Ok(rc) => {
                    let docs = rc.into_docs(component_id, &self.metadata);
                    all_inputs.extend(docs.inputs);
                    all_outputs.extend(docs.outputs);
                    all_features.extend(docs.features);
                    all_apm.extend(docs.apm);
                }
                Err(e) => log::debug!(
                    "No beat-rendered-config.yml for component {}: {}",
                    component_id,
                    e
                ),
            }
        }

        // Export beat metrics
        let summary = beat_metrics::export_beat_metrics(all_beat_docs, &self.exporter).await;
        let _ = summary_tx.send(summary).await;

        // Export input metrics
        let summary = input_metrics::export_input_metrics(all_input_docs, &self.exporter).await;
        let _ = summary_tx.send(summary).await;

        // Export rendered config splits
        let rendered = rendered_config::RenderedConfigDocs {
            inputs: all_inputs,
            outputs: all_outputs,
            features: all_features,
            apm: all_apm,
        };
        let summaries =
            rendered_config::export_rendered_configs(rendered, &self.exporter).await;
        for summary in summaries {
            let _ = summary_tx.send(summary).await;
        }

        Ok(())
    }

    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }
}

impl DiagnosticProcessor for AgentDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let agent_info = receiver.get::<AgentInfo>().await?;
        let metadata = AgentMetadata::try_new(manifest, agent_info.clone())?;
        let report = DiagnosticReportBuilder::from(metadata.diagnostic.clone())
            .product(Product::Agent)
            .receiver(receiver.to_string())
            .build()?;

        Ok((
            Box::new(Self {
                lookups: Lookups {},
                receiver,
                exporter,
                metadata,
            }),
            report,
        ))
    }

    async fn process(mut self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        log::debug!("Running Agent diagnostic processors");

        // Process root-level sources
        // state.yaml gives us component IDs for per-component processing
        let state: Option<State> = self.receiver.get::<State>().await.ok();
        let component_ids = state
            .as_ref()
            .map(|s| s.component_ids())
            .unwrap_or_default();

        if let Some(state) = state {
            let summary = state
                .documents_export(&self.exporter, &self.lookups, &self.metadata)
                .await;
            let _ = summary_tx.send(summary).await;
        }

        self.process_datasource::<ComputedConfig>(summary_tx.clone())
            .await?;
        self.process_datasource::<LocalConfig>(summary_tx.clone())
            .await?;

        // Process per-component sources
        if !component_ids.is_empty() {
            self.process_components(&component_ids, summary_tx.clone())
                .await?;
        }

        // Process logs
        if let Some(filename) = self.receiver.filename() {
            let logs_root = std::path::Path::new(&filename).join("logs");
            if logs_root.is_dir() {
                let log_files = logs::discover_log_files(&logs_root);
                if !log_files.is_empty() {
                    log::info!("Processing {} log files", log_files.len());
                    match logs::export_logs(&log_files, &self.exporter, &self.metadata).await {
                        Ok(summary) => {
                            let _ = summary_tx.send(summary).await;
                        }
                        Err(e) => log::error!("Failed to process logs: {}", e),
                    }
                }
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            self.metadata.agent_info.hostname(),
            self.metadata.agent_info.agent_id(),
            "agent".to_string(),
        )
    }
}

#[derive(Serialize)]
pub struct Lookups {}
