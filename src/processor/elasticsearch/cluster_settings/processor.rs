// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::DataStreamName;
use super::{
    super::{DataProcessor, ElasticsearchMetadata, Lookups, Metadata},
    ClusterSettings,
};
use json_patch::merge;
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT: &str = "default";
const PERSISTENT: &str = "persistent";
const TRANSIENT: &str = "transient";

impl DataProcessor<Lookups, ElasticsearchMetadata> for ClusterSettings {
    fn generate_docs(
        self,
        _lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> (String, Vec<Value>) {
        let data_stream = "settings-cluster-esdiag".to_string();
        let data_stream_name = DataStreamName::from(data_stream.as_str());
        let metadata = metadata.for_data_stream(&data_stream).as_meta_doc();

        let scopes: Vec<_> = vec![
            (DEFAULT, self.defaults),
            (TRANSIENT, self.transient),
            (PERSISTENT, self.persistent),
        ];
        log::debug!("cluster_settings scopes: {}", scopes.len());
        let cluster_settings_doc = ClusterSettingsDoc::new(metadata.clone(), data_stream_name);

        let cluster_settings: Vec<Value> = scopes.into_iter().map(|(priority, settings)| {
        let cluster_patch = json!({
            "cluster.max_shards_per_node.frozen": null,
            "cluster.max_shards_per_node": null,
            "cluster" : {
                "max_shards_per_node_frozen": settings.get("cluster.max_shards_per_node.frozen").take(),
                "max_shards_per_node": settings.get("cluster.max_shards_per_node").take(),
                "routing" :{
                    "allocation" :{
                        "disk" : {
                            "watermark" : {
                                "enable_for_single_data_node" : settings.get("cluster.routing.allocation.disk.watermark.enable_for_single_data_node").take(),
                                "flood_stage" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage").take(),
                                "flood_stage.frozen" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen").take(),
                                "flood_stage.frozen.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen.max_headroom").take(),
                                "flood_stage.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.max_headroom").take(),
                                "high" : settings.get("cluster.routing.allocation.disk.watermark.high").take(),
                                "high.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.high.max_headroom").take(),
                                "low" : settings.get("cluster.routing.allocation.disk.watermark.low").take(),
                                "low_max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.low.max_headroom").take(),
                            }
                        }

                    }
                }
            },
            "http.type": null,
            "http.type.default": null,
            "http": {
                "type.current": settings.get("http.type").take(),
                "type.default": settings.get("http.type.default").take(),
            },
            "thread_pool.estimated_time_interval.warn_threshold": null,
            "transport.type": null,
            "transport.type.default": null,
            "transport": {
                "type.current": settings.get("transport.type").take(),
                "type.default": settings.get("transport.type.default").take(),
            },
            "xpack.searchable.snapshot.shared_cache.size.max_headroom": null,
        });
        let mut cluster_settings_doc = cluster_settings_doc.clone().with(priority, settings);
        merge(&mut cluster_settings_doc.cluster, &cluster_patch);
        json!(cluster_settings_doc)
    })
    .collect();
        log::debug!("cluster_settings docs: {}", cluster_settings.len());
        (data_stream, cluster_settings)
    }
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
