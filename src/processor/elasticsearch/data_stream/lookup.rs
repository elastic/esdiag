// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::{DataStreamDocument, DataStreams, Lookup};
use super::Indices;
use eyre::Result;

impl From<&String> for Lookup<DataStreamDocument> {
    fn from(string: &String) -> Self {
        match serde_json::from_str::<DataStreams>(string) {
            Ok(data_streams) => Lookup::<DataStreamDocument>::from_parsed(data_streams),
            Err(e) => {
                tracing::warn!("Failed to parse DataStreams: {}", e);
                Lookup::new()
            }
        }
    }
}

impl From<DataStreams> for Lookup<DataStreamDocument> {
    fn from(mut data_streams: DataStreams) -> Self {
        let mut lookup = Lookup::<DataStreamDocument>::new();
        data_streams.data_streams.drain(..).for_each(|mut data_stream| {
            data_stream.build();
            let name = data_stream.name.clone();
            let mut indices: Indices = data_stream.indices.drain(..).collect();
            let write_index = indices.len() - 1;
            let write_data_stream = data_stream.clone().set_write_index(true);

            // Add base document once for all non-write indices (including failure store)
            let base_doc = DataStreamDocument::from(data_stream.clone());
            lookup.add(base_doc).with_name(&name);

            for (i, index) in indices.drain(..).enumerate() {
                if i == write_index {
                    lookup
                        .add(DataStreamDocument::from(write_data_stream.clone()))
                        .with_name(&name)
                        .with_id(&index.index_name);
                } else {
                    lookup.with_id(&index.index_name);
                }
            }

            if let Some(failure_store) = &data_stream.failure_store {
                // Re-add/re-activate base document context for failure store indices
                let base_doc = DataStreamDocument::from(data_stream.clone());
                lookup.add(base_doc);
                for index in &failure_store.indices {
                    lookup.with_id(&index.index_name);
                }
            }
        });

        tracing::debug!("lookup data_stream entries: {}", lookup.len(),);
        lookup
    }
}

impl From<Result<DataStreams>> for Lookup<DataStreamDocument> {
    fn from(data_streams: Result<DataStreams>) -> Self {
        match data_streams {
            Ok(data_streams) => Lookup::<DataStreamDocument>::from_parsed(data_streams),
            Err(e) => {
                tracing::warn!("Failed to parse DataStreams: {}", e);
                Lookup::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::processor::elasticsearch::Lookup;
    use crate::processor::elasticsearch::data_stream::DataStreamDocument;
    use crate::processor::elasticsearch::data_stream::data::DataStreams;

    #[test]
    fn test_failure_store_enrichment() {
        let json = r#"{
          "data_streams": [
            {
              "name": "metrics-index-esdiag",
              "timestamp_field": { "name": "@timestamp" },
              "indices": [
                {
                  "index_name": ".ds-metrics-index-esdiag-2025.11.20-000004",
                  "index_uuid": "uuid1"
                },
                {
                  "index_name": ".ds-metrics-index-esdiag-2025.11.21-000005",
                  "index_uuid": "uuid2"
                }
              ],
              "generation": 1,
              "status": "GREEN",
              "failure_store": {
                "enabled": true,
                "indices": [
                  {
                    "index_name": ".fs-metrics-index-esdiag-2026.02.07-000012",
                    "index_uuid": "uuid-fs"
                  }
                ]
              }
            }
          ]
        }"#;

        let data_streams: DataStreams = serde_json::from_str(json).unwrap();
        let lookup = Lookup::<DataStreamDocument>::from(data_streams);

        // Check non-write backing index
        let doc_base = lookup.by_id(".ds-metrics-index-esdiag-2025.11.20-000004").unwrap();
        assert_eq!(doc_base.name, "metrics-index-esdiag");
        assert!(!doc_base.is_write_index);

        // Check write backing index (last in the list)
        let doc_write = lookup.by_id(".ds-metrics-index-esdiag-2025.11.21-000005").unwrap();
        assert_eq!(doc_write.name, "metrics-index-esdiag");
        assert!(doc_write.is_write_index);

        // Check failure store index
        let doc_fs = lookup.by_id(".fs-metrics-index-esdiag-2026.02.07-000012").unwrap();
        assert_eq!(doc_fs.name, "metrics-index-esdiag");
        assert_eq!(doc_fs.r#type, "metrics");
        assert_eq!(doc_fs.dataset, "index");
        assert_eq!(doc_fs.namespace, "esdiag");
        assert!(!doc_fs.is_write_index);

        // Check by_name returns the write document (the last one mapped to the name)
        let doc_name = lookup.by_name("metrics-index-esdiag").unwrap();
        assert!(doc_name.is_write_index);
    }
}
