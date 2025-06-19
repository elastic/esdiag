use super::{DiagnosticProcessor, ElasticsearchDiagnostic};
use crate::{
    data::diagnostic::{
        DiagPath, DiagnosticManifest, DiagnosticReport, DiagnosticReportBuilder, Lookup, Product,
    },
    exporter::Exporter,
    receiver::Receiver,
};
use eyre::Result;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Serialize)]
pub struct ElasticCloudKubernetesDiagnostic {
    lookups: Arc<Lookups>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    report: Arc<RwLock<DiagnosticReport>>,
    included_diagnostics: Vec<DiagPath>,
}

impl DiagnosticProcessor for ElasticCloudKubernetesDiagnostic {
    async fn new(
        mut manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
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

        Ok(Box::new(Self {
            lookups,
            exporter: Arc::new(exporter),
            receiver: Arc::new(receiver),
            report: Arc::new(RwLock::new(report)),
            included_diagnostics,
        }))
    }

    async fn run(self) -> Result<DiagnosticReport> {
        self.receiver.is_connected().await;
        for diagnostic in self.included_diagnostics {
            match diagnostic.diag_type.as_str() {
                "elasticsearch" => {
                    log::info!(
                        "Processing {} diagnostic at {}",
                        diagnostic.diag_type,
                        diagnostic.diag_path
                    );
                    let receiver = self.receiver.clone_for_subdir(&diagnostic.diag_path)?;
                    let manifest = receiver.try_get_manifest().await?;
                    let diagnostic =
                        ElasticsearchDiagnostic::new(manifest, receiver, self.exporter.cloned())
                            .await?;
                    diagnostic.run().await?;
                }
                _ => {
                    log::warn!(
                        "Skipping {} diagnostic at {}",
                        diagnostic.diag_type,
                        diagnostic.diag_path
                    );
                }
            }
        }

        let report = self.report.write().await;
        self.exporter.save_report(&*report).await?;
        Ok(report.clone())
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub k8s_node: Lookup<String>,
}
