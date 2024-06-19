use super::lookup::node::NodeData;
use super::metadata::{DataStream, Metadata, MetadataDoc};
use json_patch::merge;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};

pub fn enrich(metadata: &Metadata, data: Value) -> Vec<Value> {
    let lookup = &metadata.lookup;
    let metadata = &metadata.as_doc;
    let nodes: Vec<_> = match data["nodes"].as_object() {
        Some(data) => data.iter().collect(),
        None => {
            log::error!("Failed to deserialize nodes stats");
            return Vec::new();
        }
    };
    log::debug!("nodes: {}", nodes.len());

    let node_stats_docs: Vec<Value> = nodes
        .par_iter()
        .flat_map(|(node_id, node_stats)| {
            let node_doc = json!(NodeStatsDoc {
                metadata: metadata.clone(),
                data_stream: DataStream::from("metrics-node-esdiag"),
                node: lookup.node.by_id(node_id.as_str()).cloned(),
            });

            // Extract transport.actions
            let data_stream_transport = json!({
                "data_stream": DataStream::from("metrics-node.transport.actions-esdiag"),
            });

            let transport_actions: Vec<_> = match node_stats["transport"]["actions"].as_object() {
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
                        merge(&mut action, &node_doc);
                        merge(&mut action, &data_stream_transport);
                        action
                    })
                    .collect(),
                None => Vec::new(),
            };
            log::trace!("transport_actions: {}", transport_actions.len());

            // Extract http.clients
            let data_stream_http = json!({
                "data_stream": DataStream::from("metrics-node.http.clients-esdiag"),
            });

            let clients: Vec<_> = match node_stats["http"]["clients"].as_array() {
                Some(data) => data
                    .into_iter()
                    .collect::<Vec<_>>()
                    .par_iter()
                    .map(|client| {
                        let mut doc = json!({ "http": { "client": client, }, });
                        merge(&mut doc, &node_doc);
                        merge(&mut doc, &data_stream_http);
                        doc
                    })
                    .collect(),
                None => Vec::new(),
            };
            log::trace!("clients: {}", clients.len());

            // Extract adaptive_selection
            let data_stream_adaptive = json!({
                "data_stream": DataStream::from("metrics-node.adaptive_selection-esdiag"),
            });

            let adaptive_selections: Vec<_> = match node_stats["adaptive_selection"].as_object() {
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
                                "node": lookup.node.by_id(&peer_node_id),
                            },
                        });

                        merge(&mut doc, &peer_node_patch);
                        merge(&mut doc, &node_doc);
                        merge(&mut doc, &data_stream_adaptive);
                        doc
                    })
                    .collect(),
                None => Vec::new(),
            };
            log::trace!("adaptive_selections: {}", adaptive_selections.len());

            // Extract ingest.pipelines and ingest.processors
            let data_stream_processor = json!({
                "data_stream": DataStream::from("metrics-ingest.processor-esdiag"),
            });
            let data_stream_pipeline = json!({
                "data_stream": DataStream::from("metrics-ingest.pipeline-esdiag"),
            });
            let ingest_role: Value = Value::from("ingest");
            let is_ingest = node_stats["roles"]
                .as_array()
                .expect("Failed get node.roles array")
                .contains(&ingest_role);

            let pipelines: Vec<_> = if is_ingest {
                match node_stats["ingest"]["pipelines"].as_object() {
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
                                        merge(&mut doc, &node_doc);
                                        merge(&mut doc, &data_stream_processor);
                                        doc
                                    })
                                    .collect(),
                                None => Vec::new(),
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
                            merge(&mut doc, &node_doc);
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
            log::trace!("pipelines: {}", pipelines.len());

            // Extract discovery.cluster_applier_stats.recordings dataset
            let data_stream_cluster_applier = json!({
                "data_stream": DataStream::from("metrics-node.discovery.cluster_applier-esdiag"),
            });

            let recordings: Vec<_> =
                match node_stats["discovery"]["cluster_applier_stats"]["recordings"].as_array() {
                    Some(data) => data
                        .par_iter()
                        .map(|recording| {
                            let mut doc = json!({
                                "cluster_applier_stats": recording,
                            });

                            merge(&mut doc, &node_doc);
                            merge(&mut doc, &data_stream_cluster_applier);
                            doc
                        })
                        .collect(),
                    None => Vec::new(),
                };
            log::trace!("recordings: {}", recordings.len());

            // Final node_stats document
            let mut doc = json!({
                "node": &node_stats,
                "shared_cache": lookup.shared_cache.by_id(node_id.as_str()),
            });

            // Remove extracted datasets, add enriched datasets
            let omit_patch = json!({
                "node" : {
                    "http": { "clients": null, "routes": null },
                    "adaptive_selection" : null,
                    "ingest": { "pipelines": null },
                    "discovery": { "cluster_applier_stats": null },
                    "transport": { "actions": null },
                }
            });

            merge(&mut doc, &node_doc);
            merge(&mut doc, &omit_patch);

            // Start a vec with the top-level node_stats doc
            let mut docs: Vec<Value> = vec![doc];
            docs.extend(adaptive_selections);
            docs.extend(clients);
            docs.extend(pipelines);
            docs.extend(recordings);
            docs.extend(transport_actions);
            log::trace!("node_stats docs for {}: {}", node_id, docs.len());
            docs
        })
        .collect();

    log::debug!("node_stats docs: {}", node_stats_docs.len());
    node_stats_docs
}

// Serializing data structures

#[derive(Clone, Serialize)]
struct NodeStatsDoc {
    #[serde(flatten)]
    metadata: MetadataDoc,
    data_stream: DataStream,
    node: Option<NodeData>,
}
