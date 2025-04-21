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
pub struct KubernetesPlatformDiagnostic {
    lookups: Arc<Lookups>,
    #[serde(skip)]
    exporter: Arc<Exporter>,
    #[serde(skip)]
    receiver: Arc<Receiver>,
    #[serde(skip)]
    report: Arc<RwLock<DiagnosticReport>>,
    included_diagnostics: Vec<DiagPath>,
}

impl DiagnosticProcessor for KubernetesPlatformDiagnostic {
    async fn new(
        mut manifest: DiagnosticManifest,
        receiver: Receiver,
        exporter: Exporter,
    ) -> Result<Box<Self>> {
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

        Ok(Box::new(Self {
            lookups,
            exporter: Arc::new(exporter),
            receiver: Arc::new(receiver),
            report: Arc::new(RwLock::new(report)),
            included_diagnostics,
        }))
    }

    async fn run(self) -> Result<()> {
        self.receiver.is_connected().await;
        for diagnostic in self.included_diagnostics {
            let diag_type = diagnostic.diag_type.as_str().trim_end_matches("appconfig");
            match diag_type {
                "elasticsearch" => {
                    log::info!(
                        "Processing {} diagnostic at {}",
                        diag_type,
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
                        diag_type,
                        diagnostic.diag_path
                    );
                }
            }
        }

        let report = self.report.write().await;
        log::info!(
            "Created {} documents for diagnostic (kubernetes platform): {}",
            report.docs.created,
            report.metadata.id,
        );
        self.exporter.save_report(&*report).await?;
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Lookups {
    pub k8s_node: Lookup<String>,
}
