// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    DiagnosticProcessor, ProcessorSummary,
    diagnostic::{DiagPath, DiagnosticManifest, DiagnosticMetadata, DiagnosticReport, DiagnosticReportBuilder, Lookup},
};
use crate::{data::Product, exporter::Exporter, receiver::Receiver};
use eyre::Result;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct ElasticCloudKubernetesDiagnostic {
    lookups: Arc<Lookups>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(flatten)]
    metadata: DiagnosticMetadata,
}

impl DiagnosticProcessor for ElasticCloudKubernetesDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        _exporter: Arc<Exporter>,
        manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let lookups = Arc::new(Lookups {
            k8s_node: Lookup::new(),
        });

        let report = DiagnosticReportBuilder::try_from(manifest.clone())?
            .product(Product::ECK)
            .receiver(receiver.to_string())
            .build()?;

        let metadata = DiagnosticMetadata::try_from(manifest)?;

        Ok((
            Box::new(Self {
                lookups,
                receiver,
                metadata,
            }),
            report,
        ))
    }

    async fn process(self, _summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        self.receiver.is_connected().await;
        log::warn!(
            "Elastic Cloud Kubernetes diagnostics only process included Elasticsearch bundles"
        );
        Ok(())
    }

    fn id(&self) -> &str {
        &self.metadata.id
    }

    fn origin(&self) -> (String, String, String) {
        (
            "eck".to_string(),
            "".to_string(),
            "orchestration".to_string(),
        )
    }
}

impl ElasticCloudKubernetesDiagnostic {
    pub fn uuid(&self) -> &str {
        &self.metadata.uuid
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub k8s_node: Lookup<String>,
}
