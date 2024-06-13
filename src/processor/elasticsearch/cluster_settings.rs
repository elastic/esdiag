use super::metadata::Metadata;
use json_patch::merge;
use serde_json::{json, Value};

pub fn enrich(metadata: &Metadata, data: Value) -> Vec<Value> {
    let scopes: Vec<_> = match data.as_object() {
        Some(data) => data.into_iter().collect(),
        None => return Vec::new(),
    };
    log::debug!("cluster_settings scopes: {}", scopes.len());
    let data_stream = json!({
        "data_stream": {
            "dataset": "cluster",
            "namespace": "esdiag",
            "type": "settings",
        }
    });
    let cluster_settings: Vec<Value> = scopes
        .iter()
        .map(|(priority, settings)| {
            let mut doc = json!({
                "@timestamp": metadata.diagnostic.collection_date,
                "cluster": metadata.cluster,
                "diagnostic": metadata.diagnostic,
                "priority": priority,
            });
            let shards_per_node_patch = json!({
                "cluster": {
                    "max_shards_per_node_frozen": settings["cluster.max_shards_per_node.frozen"].clone(),
                    "max_shards_per_node": settings["cluster.max_shards_per_node"].clone(),
                },
                "cluster.max_shards_per_node.frozen": null,
                "cluster.max_shards_per_node:": null,
            });
            let watermark_patch = json!({
                "cluster" : {
                    "routing" :{
                        "allocation" :{
                            "disk" : {
                                "watermark" : {
                                    "enable_for_single_data_node" : settings.get("cluster.routing.allocation.disk.watermark.enable_for_single_data_node").unwrap_or(&Value::Null).clone(),
                                    "flood_stage" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage").unwrap_or(&Value::Null).clone(),
                                    "flood_stage.frozen" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen").unwrap_or(&Value::Null).clone(),
                                    "flood_stage.frozen.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.frozen.max_headroom").unwrap_or(&Value::Null).clone(),
                                    "flood_stage.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.flood_stage.max_headroom").unwrap_or(&Value::Null).clone(),
                                    "high" : settings.get("cluster.routing.allocation.disk.watermark.high").unwrap_or(&Value::Null).clone(),
                                    "high.max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.high.max_headroom").unwrap_or(&Value::Null).clone(),
                                    "low" : settings.get("cluster.routing.allocation.disk.watermark.low").unwrap_or(&Value::Null).clone(),
                                    "low_max_headroom" : settings.get("cluster.routing.allocation.disk.watermark.low.max_headroom").unwrap_or(&Value::Null).clone(),
                                }
                            }

                        }
                    }
                },
                "cluster.routing.allocation.disk.watermark.enable_for_single_data_node": null,
                "cluster.routing.allocation.disk.watermark.flood_stage": null,
                "cluster.routing.allocation.disk.watermark.flood_stage.frozen": null,
                "cluster.routing.allocation.disk.watermark.flood_stage.frozen.max_headroom": null,
                "cluster.routing.allocation.disk.watermark.flood_stage.max_headroom": null,
                "cluster.routing.allocation.disk.watermark.high": null,
                "cluster.routing.allocation.disk.watermark.high.max_headroom": null,
                "cluster.routing.allocation.disk.watermark.low": null,
                "cluster.routing.allocation.disk.watermark.low.max_headroom": null,
            });
            merge(&mut doc, &settings);
            merge(&mut doc, &data_stream);
            merge(&mut doc, &watermark_patch);
            merge(&mut doc, &shards_per_node_patch);
            doc
        })
        .collect();

    log::debug!("cluster_settings docs: {}", cluster_settings.len());
    cluster_settings
}
