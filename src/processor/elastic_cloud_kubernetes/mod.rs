// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Bundle discovery, source table, and lookups
mod bundle;
/// Kubernetes event documents
mod event;
/// Shared document fragments
mod fields;
/// Pod label and template helpers
mod labels;
/// Operator and tool log parsing
mod logs;
/// ECK document envelope
mod metadata;
/// Kubernetes node documents
mod node;
/// Operator summary document
mod operator;
/// Operator log documents
mod operator_log;
/// Pod and container documents
mod pod;
/// Kubernetes resource quantity parsing
mod quantity;
/// ECK custom resource and nodeSet documents
mod resource;
/// Allowlisted kubectl resource structs
mod resources;
/// Volume documents
mod volume;

use super::{
    DiagnosticProcessor, DocumentExporter, ProcessorSummary,
    api::ProcessSelection,
    diagnostic::{DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder},
};
use crate::{exporter::Exporter, receiver::Receiver};
use bundle::EckBundle;
use eyre::Result;
use metadata::EckMetadata;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct ElasticCloudKubernetesDiagnostic {
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(flatten)]
    metadata: EckMetadata,
}

impl DiagnosticProcessor for ElasticCloudKubernetesDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
        _process_selection: Option<ProcessSelection>,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        // A platform-only diagnostic: no application axis (ADR-0001); the
        // platform (ECK) rides on the manifest and identifiers.
        let report = DiagnosticReportBuilder::try_from(manifest)?
            .receiver(receiver.to_string())
            .build()?;
        // Included children join on the report's id, so documents must use
        // the same metadata rather than deriving a new id from the manifest.
        let metadata = EckMetadata::new(report.diagnostic.metadata.clone());

        Ok((
            Box::new(Self {
                exporter,
                receiver,
                metadata,
            }),
            report,
        ))
    }

    async fn process(self, summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        let bundle = EckBundle::discover(self.receiver.clone()).await?;
        tracing::debug!("ECK bundle namespaces: {:?}", bundle.namespaces());
        bundle
            .index()
            .await?
            .export(&self.exporter, &self.metadata, &summary_tx)
            .await
    }

    fn id(&self) -> &str {
        &self.metadata.diagnostic.id
    }

    fn origin(&self) -> (String, String, String) {
        ("eck".to_string(), "".to_string(), "platform".to_string())
    }
}

impl ElasticCloudKubernetesDiagnostic {
    #[cfg(test)]
    pub fn uuid(&self) -> &str {
        &self.metadata.diagnostic.uuid
    }
}
