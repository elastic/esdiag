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

#[derive(Clone, Serialize)]
pub struct KubernetesPlatformDiagnostic {
    pub lookups: Arc<Lookups>,
    #[serde(skip)]
    pub exporter: Arc<Exporter>,
    #[serde(skip)]
    pub receiver: Arc<Receiver>,
    #[serde(skip)]
    pub included_diagnostics: Vec<DiagPath>,
}

impl DiagnosticProcessor for KubernetesPlatformDiagnostic {
    async fn try_new(
        receiver: Arc<Receiver>,
        exporter: Arc<Exporter>,
        mut manifest: DiagnosticManifest,
    ) -> Result<(Box<Self>, DiagnosticReport)> {
        let lookups = Arc::new(Lookups {
            k8s_node: Lookup::new(),
        });

        log::debug!(
            "Kubernetes platform diagnostic includes: {:?}",
            &manifest.included_diagnostics
        );

        let included_diagnostics = match manifest.included_diagnostics.take() {
            Some(diags) => diags,
            None => vec![],
        };

        let report = DiagnosticReportBuilder::try_from(manifest)?
            .product(Product::KubernetesPlatform)
            .receiver(receiver.to_string())
            .build()?;

        Ok((
            Box::new(Self {
                lookups,
                exporter,
                receiver,
                included_diagnostics,
            }),
            report,
        ))
    }

    async fn process(self, _summary_tx: mpsc::Sender<ProcessorSummary>) -> Result<()> {
        self.receiver.is_connected().await;
        log::warn!("Kubernetes Platform diagnostics only process included Elasticsearch bundles");
        Ok(())
    }

    fn id(&self) -> &str {
        "undefined"
    }

    fn origin(&self) -> (String, String, String) {
        (
            "mki".to_string(),
            "".to_string(),
            "orchestration".to_string(),
        )
    }
}

impl KubernetesPlatformDiagnostic {
    pub fn cloned_receiver(&self, next: &DiagPath) -> Result<Receiver> {
        self.receiver.clone_for_subdir(&next.diag_path)
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub k8s_node: Lookup<String>,
}
