// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, ProcessorSummary};
use super::{HealthDiagnosis, HealthImpact, HealthIndicator, HealthReport};
use crate::exporter::Exporter;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, value::RawValue};

impl DocumentExporter<Lookups, ElasticsearchMetadata> for HealthReport {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        tracing::debug!("processing pending tasks");
        let metadata_indicator = metadata.for_data_stream("health-indicator-esdiag").as_meta_doc();
        let metadata_impact = metadata.for_data_stream("health-impact-esdiag").as_meta_doc();
        let metadata_diagnosis = metadata.for_data_stream("health-diagnosis-esdiag").as_meta_doc();

        let mut indicators: Vec<(String, HealthIndicator)> = self.indicators.into_par_iter().collect();

        let health_docs: Vec<Value> = indicators
            .par_drain(..)
            .flat_map(|(name, mut indicator)| {
                let mut docs: Vec<Value> = Vec::with_capacity(10);
                let named_health_indicator = NamedHealthIndicator::new(name, &indicator);

                if let Some(mut impacts) = indicator.impacts.take() {
                    impacts.drain(..).for_each(|impact| {
                        if let Ok(value) = serde_json::to_value(HealthImpactDoc {
                            health: Impact {
                                indicator: named_health_indicator.clone(),
                                impact,
                            },
                            metadata: metadata_impact.clone(),
                        }) {
                            docs.push(value)
                        }
                    });
                };

                if let Some(mut diagnosis) = indicator.diagnosis.take() {
                    diagnosis.drain(..).for_each(|diagnosis| {
                        if let Ok(value) = serde_json::to_value(HealthDiagnosisDoc {
                            health: Diagnosis {
                                diagnosis,
                                indicator: named_health_indicator.clone(),
                            },
                            metadata: metadata_diagnosis.clone(),
                        }) {
                            docs.push(value)
                        }
                    });
                }

                if let Ok(value) = serde_json::to_value(HealthIndicatorDoc {
                    health: named_health_indicator,
                    metadata: metadata_indicator.clone(),
                }) {
                    docs.push(value)
                };
                docs
            })
            .collect();

        tracing::debug!("Health report docs: {}", health_docs.len());
        let mut summary = ProcessorSummary::new("health-indicator-esdiag".to_string());
        match exporter.send("health-indicator-esdiag".to_string(), health_docs).await {
            Ok(batch) => summary.add_batch(batch),
            Err(err) => tracing::error!("Failed to send health report: {}", err),
        }
        summary
    }
}

#[derive(Clone, Serialize)]
struct NamedHealthIndicator {
    status: String,
    symptom: String,
    details: Option<Box<RawValue>>,
    indicator: String,
}

impl NamedHealthIndicator {
    fn new(name: String, indicator: &HealthIndicator) -> Self {
        NamedHealthIndicator {
            indicator: name,
            symptom: indicator.symptom.clone(),
            details: indicator.details.clone(),
            status: indicator.status.clone(),
        }
    }
}

#[derive(Serialize)]
struct HealthIndicatorDoc {
    health: NamedHealthIndicator,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct HealthImpactDoc {
    health: Impact,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct Impact {
    #[serde(flatten)]
    indicator: NamedHealthIndicator,
    impact: HealthImpact,
}

#[derive(Serialize)]
struct HealthDiagnosisDoc {
    health: Diagnosis,
    #[serde(flatten)]
    metadata: Value,
}

#[derive(Serialize)]
struct Diagnosis {
    #[serde(flatten)]
    indicator: NamedHealthIndicator,
    diagnosis: HealthDiagnosis,
}
