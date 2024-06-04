use super::metadata::Metadata;
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{json, Value};

pub async fn enrich(metadata: &Metadata, data: Value) -> Vec<Value> {
    let nodes: Vec<_> = match data["nodes"].as_object() {
        Some(data) => data.iter().collect(),
        None => return Vec::new(),
    };
    log::debug!("nodes: {}", nodes.len());

    //for (node_id, node) in nodes_stats_data {
    let nodes_stats: Vec<Value> = nodes
        .par_iter()
        .flat_map(|(node_id, node)| {
            let metadata_patch = json!({
                "@timestamp": metadata.diagnostic.collection_date,
                "cluster": metadata.cluster,
                "diagnostic": metadata.diagnostic,
                "node": metadata.lookup.node.by_id(node_id.as_str()),
            });

            // Extract transport.actions
            let data_stream_transport = json!({
                "data_stream": {
                    "dataset": "node.transport.actions",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });

            let transport_actions: Vec<_> = match node["transport"]["actions"].as_object() {
                Some(data) => data
                    .into_iter()
                    .collect::<Vec<_>>()
                    .par_iter()
                    .map(|(name, action)| {
                        let mut action = json!({
                            "transport": {
                                "action": action,
                            },
                        });

                        let action_patch = json!({
                            "transport": {
                                "action": {
                                    "name": name,
                                },
                            },
                        });

                        merge(&mut action, &action_patch);
                        merge(&mut action, &metadata_patch);
                        merge(&mut action, &data_stream_transport);
                        action
                    })
                    .collect(),
                None => Vec::new(),
            };

            // Extract http.clients
            let data_stream_http = json!({
                "data_stream": {
                    "dataset": "node.http.clients",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });

            let clients: Vec<_> = match node["http"]["clients"].as_array() {
                Some(data) => data
                    .into_iter()
                    .collect::<Vec<_>>()
                    .par_iter()
                    .map(|client| {
                        let mut doc = json!({ "http": { "client": client, }, });
                        merge(&mut doc, &metadata_patch);
                        merge(&mut doc, &data_stream_http);
                        doc
                    })
                    .collect(),
                None => return Vec::new(),
            };

            // Extract adaptive_selection
            let data_stream_adaptive = json!({
                "data_stream": {
                    "dataset": "node.adaptive_selection",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });

            //for (peer_node_id, adaptive_selection) in node["adaptive_selection"].as_object().unwrap() {

            let adaptive_selections: Vec<_> = match node["adaptive_selection"].as_object() {
                Some(data) => data
                    .into_iter()
                    .collect::<Vec<_>>()
                    .par_iter()
                    .map(|(peer_node_id, adaptive_selection)| {
                        let mut doc = json!({
                            "adaptive_selection": adaptive_selection,
                        });

                        let peer_node_patch = json!({
                            "adaptive_selection": {
                                "node": metadata.lookup.node.by_id(peer_node_id.as_str()),
                            },
                        });

                        merge(&mut doc, &peer_node_patch);
                        merge(&mut doc, &metadata_patch);
                        merge(&mut doc, &data_stream_adaptive);
                        doc
                    })
                    .collect(),
                None => Vec::new(),
            };

            // Extract ingest.pipelines and ingest.processors
            let data_stream_processor = json!({
                "data_stream": {
                    "dataset": "ingest.processor",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });
            let data_stream_pipeline = json!({
                "data_stream": {
                    "dataset": "ingest.pipeline",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });
            let ingest_role: Value = Value::from("ingest");
            let is_ingest = node["roles"].as_array().unwrap().contains(&ingest_role);

            let pipelines: Vec<_> = if is_ingest {
                match node["ingest"]["pipelines"].as_object() {
                    Some(data) => data
                        .into_iter()
                        .collect::<Vec<_>>()
                        .par_iter()
                        .flat_map(|(name, pipeline)| {
                            let processors: Vec<_> = match pipeline["processors"].as_array() {
                                Some(data) => data
                                    .par_iter()
                                    .enumerate()
                                    .map(|(index, processor)| {
                                        let mut doc = json!({
                                            "ingest": {
                                                "pipeline": {
                                                    "name": name,
                                                },
                                                "processor": processor,
                                            },
                                        });

                                        let processor_patch = json!({
                                            "ingest": {
                                                "processor": {
                                                    "order": index,
                                                }
                                            }
                                        });

                                        merge(&mut doc, &processor_patch);
                                        merge(&mut doc, &metadata_patch);
                                        merge(&mut doc, &data_stream_processor);
                                        doc
                                    })
                                    .collect(),
                                None => return Vec::new(),
                            };

                            let mut doc = json!({
                                "ingest": {
                                    "pipeline": pipeline,
                                },
                            });

                            let pipeline = json!({
                                "ingest": {
                                    "pipeline": {
                                        "processors": null,
                                        "name": name,
                                    }
                                }
                            });

                            merge(&mut doc, &pipeline);
                            merge(&mut doc, &metadata_patch);
                            merge(&mut doc, &data_stream_pipeline);
                            let mut docs: Vec<Value> = vec![doc];
                            docs.extend(processors);
                            docs
                        })
                        .collect(),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };

            // Extract discovery.cluster_applier_stats.recordings dataset
            let data_stream_cluster_applier = json!({
                "data_stream": {
                    "dataset": "node.discovery.cluster_applier",
                    "namespace": "esdiag",
                    "type": "metrics",
                }
            });

            //for recording in node["discovery"]["cluster_applier_stats"]["recordings"]
            let recordings: Vec<_> =
                match node["discovery"]["cluster_applier_stats"]["recordings"].as_array() {
                    Some(data) => data
                        .par_iter()
                        .map(|recording| {
                            let mut doc = json!({
                                "cluster_applier_stats": recording,
                            });

                            merge(&mut doc, &metadata_patch);
                            merge(&mut doc, &data_stream_cluster_applier);
                            doc
                        })
                        .collect(),
                    None => Vec::new(),
                };

            // Final node_stats document
            let mut doc = json!({
                "data_stream": {
                    "dataset": "node",
                    "namespace": "esdiag",
                    "type": "metrics",
                },
                "node": node,
            });

            // Remove extracted datasets
            let omit_patch = json!({
                "node" : {
                    "http": { "clients": null },
                    "adaptive_selection" : null,
                    "ingest": { "pipelines": null },
                    "discovery": { "cluster_applier_stats": null },
                    "transport": { "actions": null },
                }
            });

            merge(&mut doc, &omit_patch);
            merge(&mut doc, &metadata_patch);
            let mut docs: Vec<Value> = vec![doc];
            docs.extend(adaptive_selections);
            docs.extend(clients);
            docs.extend(pipelines);
            docs.extend(recordings);
            docs.extend(transport_actions);
            docs
        })
        .collect();

    log::debug!("node_stats docs: {}", nodes_stats.len());
    nodes_stats
}
