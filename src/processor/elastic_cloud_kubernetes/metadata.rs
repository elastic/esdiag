// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::diagnostic::{DataStreamName, DiagnosticMetadata};
use crate::data::Platform;
use serde::Serialize;
use serde_json::{Value, json};

/// The envelope every ECK platform document carries. Platform documents have
/// no `diagnostic.application` (ADR-0001).
#[derive(Clone, Serialize)]
pub struct EckMetadata {
    pub diagnostic: DiagnosticMetadata,
    pub platform: Platform,
}

impl EckMetadata {
    pub fn new(diagnostic: DiagnosticMetadata) -> Self {
        Self {
            diagnostic,
            platform: Platform::ECK,
        }
    }

    pub fn collection_date(&self) -> u64 {
        self.diagnostic.collection_date
    }

    /// `body` wrapped in the envelope for `data_stream`. A `@timestamp` in
    /// `body` replaces the collection date.
    pub fn document(&self, data_stream: &str, body: impl Serialize) -> Value {
        let mut diagnostic = json!(self.diagnostic);
        diagnostic["platform"] = json!(self.platform);
        let mut doc = json!({
            "@timestamp": self.diagnostic.collection_date,
            "diagnostic": diagnostic,
            "data_stream": DataStreamName::from(data_stream),
        });
        if let (Value::Object(doc), Value::Object(body)) = (&mut doc, json!(body)) {
            doc.extend(body.into_iter().filter(|(_, value)| !value.is_null()));
        }
        doc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_has_platform_without_application_and_keeps_body_timestamp() {
        let metadata = EckMetadata::new(DiagnosticMetadata::new(
            1_000,
            "eck@2026-01-15~abcd".to_string(),
            "eck-diagnostics".to_string(),
            "uuid".to_string(),
        ));
        let doc = metadata.document(
            "logs-eck.operator-esdiag",
            json!({"@timestamp": "2026-01-15T09:55:00Z", "message": "hi", "absent": null}),
        );
        assert_eq!(doc["diagnostic"]["platform"], "elastic-cloud-kubernetes");
        assert!(doc["diagnostic"].get("application").is_none());
        assert_eq!(doc["diagnostic"]["id"], "eck@2026-01-15~abcd");
        assert_eq!(doc["@timestamp"], "2026-01-15T09:55:00Z");
        assert_eq!(doc["data_stream"]["dataset"], "eck.operator");
        assert!(doc.get("absent").is_none());

        let doc = metadata.document("settings-eck.resource-esdiag", json!({}));
        assert_eq!(doc["@timestamp"], 1_000);
    }
}
