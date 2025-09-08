// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    HealthDiagnosis, HealthImpact, HealthIndicator, HealthReport,
};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

impl DataProcessor<Lookups, ElasticsearchMetadata> for HealthReport {
    fn generate_docs(
        self,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> (String, Vec<Value>) {
        log::debug!("processing pending tasks");
        let metadata_indicator = metadata
            .for_data_stream(&"health-indicator-esdiag".to_string())
            .as_meta_doc();
        let metadata_impact = metadata
            .for_data_stream(&"health-impact-esdiag".to_string())
            .as_meta_doc();
        let metadata_diagnosis = metadata
            .for_data_stream(&"health-diagnosis-esdiag".to_string())
            .as_meta_doc();

        let mut indicators: Vec<(String, HealthIndicator)> =
            self.indicators.into_par_iter().collect();

        let health_docs: Vec<Value> = indicators
            .par_drain(..)
            .flat_map(|(name, mut indicator)| {
                let mut docs: Vec<Value> = Vec::with_capacity(10);
                let named_health_indicator = NamedHealthIndicator::new(name, &indicator);

                if let Some(mut impacts) = indicator.impacts.take() {
                    impacts.drain(..).for_each(|impact| {
                        match serde_json::to_value(HealthImpactDoc {
                            health: Impact {
                                indicator: named_health_indicator.clone(),
                                impact,
                            },
                            metadata: metadata_impact.clone(),
                        }) {
                            Ok(value) => docs.push(value),
                            Err(_) => {}
                        }
                    });
                };

                if let Some(mut diagnosis) = indicator.diagnosis.take() {
                    diagnosis.drain(..).for_each(|diagnosis| {
                        match serde_json::to_value(HealthDiagnosisDoc {
                            health: Diagnosis {
                                diagnosis,
                                indicator: named_health_indicator.clone(),
                            },
                            metadata: metadata_diagnosis.clone(),
                        }) {
                            Ok(value) => docs.push(value),
                            Err(_) => {}
                        }
                    });
                }

                match serde_json::to_value(HealthIndicatorDoc {
                    health: named_health_indicator,
                    metadata: metadata_indicator.clone(),
                }) {
                    Ok(value) => docs.push(value),
                    Err(_) => {}
                };
                docs
            })
            .collect();

        log::debug!("Health report docs: {}", health_docs.len());
        ("health-indicator-esdiag".to_string(), health_docs)
    }
}

#[derive(Clone, Serialize)]
struct NamedHealthIndicator {
    status: String,
    symptom: String,
    details: Option<Value>,
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
