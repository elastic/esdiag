// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    DiagnosticProcessor, ProcessorSummary,
    diagnostic::{
        DiagPath, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder, Lookup, Product,
    },
};
use crate::{exporter::Exporter, receiver::Receiver};
use eyre::Result;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct ElasticCloudKubernetesDiagnostic {
    pub lookups: Arc<Lookups>,
    #[serde(skip)]
    pub exporter: Arc<Exporter>,
    #[serde(skip)]
    pub receiver: Arc<Receiver>,
    pub included_diagnostics: Vec<DiagPath>,
}

impl DiagnosticProcessor for ElasticCloudKubernetesDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        mut manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let lookups = Arc::new(Lookups {
            k8s_node: Lookup::new(),
        });

        log::debug!(
            "Eck diagnostic includes: {:?}",
            &manifest.included_diagnostics
        );

        let included_diagnostics = match manifest.included_diagnostics.take() {
            Some(diags) => diags,
            None => vec![],
        };

        let report = DiagnosticReportBuilder::try_from(manifest)?
            .product(Product::ECK)
            .receiver(receiver.to_string())
            .build()?;

        Ok((
            Box::new(Self {
                lookups,
                receiver,
                exporter,
                included_diagnostics,
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
        "undefined"
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
    pub fn cloned_receiver(&self, next: &DiagPath) -> Result<Receiver> {
        self.receiver.clone_for_subdir(&next.diag_path)
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub k8s_node: Lookup<String>,
}
