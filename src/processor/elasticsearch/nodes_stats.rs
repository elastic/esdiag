use super::{DataProcessor, ElasticsearchDiagnostic, Receiver};
use crate::{data::elasticsearch::NodesStats, processor::Metadata};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct NodesStatsProcessor {
    diagnostic: Arc<ElasticsearchDiagnostic>,
    receiver: Arc<Receiver>,
}

impl NodesStatsProcessor {
    fn new(diagnostic: Arc<ElasticsearchDiagnostic>, receiver: Arc<Receiver>) -> Self {
        NodesStatsProcessor {
            diagnostic,
            receiver,
        }
    }
}

impl From<Arc<ElasticsearchDiagnostic>> for NodesStatsProcessor {
    fn from(diagnostic: Arc<ElasticsearchDiagnostic>) -> Self {
        NodesStatsProcessor::new(diagnostic.clone(), diagnostic.receiver.clone())
    }
}

impl DataProcessor for NodesStatsProcessor {
    async fn process(&self) -> (String, Vec<Value>) {
        let data_stream = "metrics-node-esdiag".to_string();
        let node_stats_metadata = self
            .diagnostic
            .metadata
            .for_data_stream(&data_stream)
            .as_meta_doc();
        let lookup_node = &self.diagnostic.lookups.node;
        let lookup_shared_cache = &self.diagnostic.lookups.shared_cache;
        let mut nodes_stats = match self.receiver.get::<NodesStats>().await {
            Ok(nodes) => nodes.nodes,
            Err(e) => {
                log::error!("Failed to deserialize nodes stats: {e}");
                return (data_stream, Vec::new());
            }
        };
        log::debug!("nodes: {}", nodes_stats.len());

        let node_stats_docs: Vec<Value> = nodes_stats
            .par_drain()
            .flat_map(|(node_id, node_stats)| {
                // Extract transport.actions
                let transport_actions_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.transport.actions-esdiag")
                    .as_meta_doc();

                let transport_actions: Vec<_> = match node_stats.transport["actions"].as_object() {
                    Some(data) => data
                        .into_iter()
                        .collect::<Vec<_>>()
                        .par_drain(..)
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
                            merge(&mut action, &transport_actions_metadata);
                            action
                        })
                        .collect(),
                    None => Vec::new(),
                };
                log::trace!("transport_actions: {}", transport_actions.len());

                // Extract http.clients
                let data_stream_http_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.http.clients-esdiag")
                    .as_meta_doc();

                let clients: Vec<_> = match node_stats.http["clients"].as_array() {
                    Some(data) => data
                        .into_iter()
                        .collect::<Vec<_>>()
                        .par_drain(..)
                        .map(|client| {
                            let mut doc = json!({ "http": { "client": client, }, });
                            merge(&mut doc, &data_stream_http_metadata);
                            doc
                        })
                        .collect(),
                    None => Vec::new(),
                };
                log::trace!("clients: {}", clients.len());

                // Extract adaptive_selection
                let adaptive_selection_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.adaptive_selection-esdiag")
                    .as_meta_doc();

                let adaptive_selections: Vec<_> = match node_stats.adaptive_selection.as_object() {
                    Some(data) => data
                        .into_iter()
                        .collect::<Vec<_>>()
                        .par_drain(..)
                        .map(|(peer_node_id, adaptive_selection)| {
                            let mut doc = json!({
                                "adaptive_selection": adaptive_selection,
                            });

                            let peer_node_patch = json!({
                                "adaptive_selection": {
                                    "node": lookup_node.by_id(&peer_node_id),
                                },
                            });

                            merge(&mut doc, &peer_node_patch);
                            merge(&mut doc, &adaptive_selection_metadata);
                            doc
                        })
                        .collect(),
                    None => Vec::new(),
                };
                log::trace!("adaptive_selections: {}", adaptive_selections.len());

                // Extract ingest.pipelines and ingest.processors
                let ingest_processor_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.ingest.processor-esdiag")
                    .as_meta_doc();
                let ingest_pipeline_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.ingest.pipeline-esdiag")
                    .as_meta_doc();

                let ingest_role = String::from("ingest");
                let is_ingest = node_stats.roles.contains(&ingest_role);

                let pipelines: Vec<_> = if is_ingest {
                    match node_stats.ingest["pipelines"].as_object() {
                        Some(data) => data
                            .into_iter()
                            .collect::<Vec<_>>()
                            .par_drain(..)
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
                                            merge(&mut doc, &ingest_processor_metadata);
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
                                merge(&mut doc, &ingest_pipeline_metadata);
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
                let cluster_applier_metadata = self
                    .diagnostic
                    .metadata
                    .for_data_stream("metrics-node.discovery.cluster_applier-esdiag")
                    .as_meta_doc();

                let recordings: Vec<_> =
                    match node_stats.discovery["cluster_applier_stats"]["recordings"].as_array() {
                        Some(data) => data
                            .par_iter()
                            .map(|recording| {
                                let mut doc = json!({
                                    "cluster_applier_stats": recording,
                                });

                                merge(&mut doc, &cluster_applier_metadata);
                                doc
                            })
                            .collect(),
                        None => Vec::new(),
                    };
                log::trace!("recordings: {}", recordings.len());

                // Final node_stats document
                let mut doc = json!({
                    "node": &node_stats,
                    "shared_cache": lookup_shared_cache.by_id(node_id.as_str()),
                });

                let omit_patch = json!({
                    "node" : {
                        "http": { "clients": null, "routes": null },
                        "ingest": { "pipelines": null },
                        "discovery": { "cluster_applier_stats": null },
                        "transport": { "actions": null },
                    }
                });

                let node_summary_patch = json!({"node": lookup_node.by_id(&node_id)});

                merge(&mut doc, &node_stats_metadata);
                merge(&mut doc, &node_summary_patch);
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
        (data_stream, node_stats_docs)
    }
}
