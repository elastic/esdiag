// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataStreamName;
use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups};
use super::{ClusterSettings, ClusterSettingsDefaults};
use crate::{exporter::Exporter, processor::ProcessorSummary};
use json_patch::merge;
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT: &str = "default";
const PERSISTENT: &str = "persistent";
const TRANSIENT: &str = "transient";

impl DocumentExporter<Lookups, ElasticsearchMetadata> for ClusterSettings {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        export_cluster_settings_docs(
            exporter,
            metadata,
            vec![(TRANSIENT, self.transient), (PERSISTENT, self.persistent)],
        )
        .await
    }
}

impl DocumentExporter<Lookups, ElasticsearchMetadata> for ClusterSettingsDefaults {
    async fn documents_export(
        self,
        exporter: &Exporter,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        export_cluster_settings_docs(
            exporter,
            metadata,
            vec![
                (DEFAULT, self.defaults),
                (TRANSIENT, self.transient),
                (PERSISTENT, self.persistent),
            ],
        )
        .await
    }
}

async fn export_cluster_settings_docs(
    exporter: &Exporter,
    metadata: &ElasticsearchMetadata,
    scopes: Vec<(&'static str, Value)>,
) -> ProcessorSummary {
    let data_stream = "settings-cluster-esdiag".to_string();
    let data_stream_name = DataStreamName::from(data_stream.as_str());
    let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

    tracing::debug!("cluster_settings scopes: {}", scopes.len());
    let cluster_settings_doc = ClusterSettingsDoc::new(metadata.clone(), data_stream_name);

    let cluster_settings: Vec<Value> = scopes
        .into_iter()
        .map(|(priority, settings)| {
            let cluster_patch = json!({
                "cluster.max_shards_per_node.frozen": null,
                "cluster.max_shards_per_node": null,
                "cluster" : {
                    "max_shards_per_node_frozen": settings.get("cluster.max_shards_per_node.frozen"),
                    "max_shards_per_node": settings.get("cluster.max_shards_per_node"),
                    "routing" :{
                        "allocation" :{
                            "disk" : {
                                "watermark" : {
                                    "enable_for_single_data_node" : settings.get("cluster.routing.allocation.disk.watermark.enable_for_single_data_node"),
                                    "flood_stage" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage"),
                                    "flood_stage.frozen" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen"),
                                    "flood_stage.frozen.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen.max_headroom"),
                                    "flood_stage.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.max_headroom"),
                                    "high" : settings.get("cluster.routing.allocation.disk.watermark.high"),
                                    "high.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.high.max_headroom"),
                                    "low" : settings.get("cluster.routing.allocation.disk.watermark.low"),
                                    "low_max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.low.max_headroom"),
                                }
                            }

                        }
                    }
                },
                "http.type": null,
                "http.type.default": null,
                "http": {
                    "type.current": settings.get("http.type"),
                    "type.default": settings.get("http.type.default"),
                },
                "thread_pool.estimated_time_interval.warn_threshold": null,
                "transport.type": null,
                "transport.type.default": null,
                "transport": {
                    "type.current": settings.get("transport.type"),
                    "type.default": settings.get("transport.type.default"),
                },
                "xpack.searchable.snapshot.shared_cache.size.max_headroom": null,
            });
            let mut cluster_settings_doc = cluster_settings_doc.clone().with(priority, settings);
            merge(&mut cluster_settings_doc.cluster, &cluster_patch);
            json!(cluster_settings_doc)
        })
        .collect();
    tracing::debug!("cluster_settings docs: {}", cluster_settings.len());
    let mut summary = ProcessorSummary::new(data_stream.clone());
    match exporter.send(data_stream, cluster_settings).await {
        Ok(batch) => summary.add_batch(batch),
        Err(err) => tracing::error!("Failed to send cluster settings: {}", err),
    }
    summary
}

// Serializing data structures

#[derive(Clone, Serialize)]
struct ClusterSettingsDoc {
    #[serde(flatten)]
    metadata: Value,
    data_stream: DataStreamName,
    priority: &'static str,
    #[serde(flatten)]
    cluster: Value,
}

impl ClusterSettingsDoc {
    pub fn new(metadata: Value, data_stream: DataStreamName) -> Self {
        ClusterSettingsDoc {
            data_stream,
            metadata,
            priority: "none",
            cluster: Value::Null,
        }
    }
    pub fn with(mut self, priority: &'static str, settings: Value) -> Self {
        self.priority = priority;
        self.cluster = settings;
        self
    }
}
