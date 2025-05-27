mod adaptive_selections;
mod cluster_applier_stats;
mod http_clients;
mod ingest_pipelines;
mod transport_actions;

use super::{DataProcessor, ElasticsearchMetadata, Lookups, NodeDocument};
use crate::{data::elasticsearch::NodesStats, processor::Metadata};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, LazyLock};

static INGEST_ROLE: LazyLock<String> = LazyLock::new(|| String::from("ingest"));

impl DataProcessor<Lookups, ElasticsearchMetadata> for NodesStats {
    fn generate_docs(
        self,
        lookups: Arc<Lookups>,
        metadata: Arc<ElasticsearchMetadata>,
    ) -> (String, Vec<Value>) {
        let mut nodes_stats = self.nodes;
        log::debug!("nodes: {}", nodes_stats.len());
        let data_stream = "metrics-node-esdiag".to_string();
        let node_stats_metadata = metadata.for_data_stream(&data_stream).as_meta_doc();
        let lookup_node = &lookups.node;
        let lookup_shared_cache = &lookups.shared_cache;

        let node_stats_docs: Vec<Value> = nodes_stats
            .par_drain()
            .flat_map(|(node_id, mut node_stats)| {
                let node_summary = lookup_node.by_id(&node_id);
                let allocated_processors = node_summary
                    .map(|node| node.os.allocated_processors)
                    .unwrap_or(1);
                node_stats.calculate_stats(allocated_processors);

                let transport_actions_docs = match node_stats.transport {
                    Some(ref mut transport) => transport_actions::extract(
                        transport["actions"].take(),
                        &metadata,
                        node_summary,
                    ),
                    None => {
                        log::trace!("Skipping transport stats for node {}", node_id);
                        Vec::new()
                    }
                };

                let http_clients_docs = http_clients::extract(
                    node_stats.http["clients"].take(),
                    &metadata,
                    node_summary,
                );

                let adaptive_selection_docs = adaptive_selections::extract(
                    node_stats.adaptive_selection.take(),
                    &metadata,
                    node_summary,
                    lookup_node,
                );

                let recording_docs = cluster_applier_stats::extract(
                    node_stats.discovery["cluster_applier_stats"].take(),
                    &metadata,
                    node_summary,
                );

                let ingest_pipelines_docs = match node_stats.roles.contains(&*INGEST_ROLE) {
                    true => ingest_pipelines::extract(
                        node_stats.ingest.pipelines.take(),
                        &metadata,
                        node_summary,
                    ),
                    false => Vec::new(),
                };

                // Final node_stats document
                let mut doc = json!({
                    "node": &node_stats,
                    "shared_cache": lookup_shared_cache.by_id(node_id.as_str()),
                });

                let omit_patch = json!({
                    "node" : {
                        "http": { "routes": null },
                    }
                });

                let node_summary_patch = json!({"node": node_summary});

                merge(&mut doc, &node_stats_metadata);
                merge(&mut doc, &node_summary_patch);
                merge(&mut doc, &omit_patch);

                // Start a vec with the top-level node_stats doc
                let mut docs: Vec<Value> = vec![doc];
                docs.extend(adaptive_selection_docs);
                docs.extend(http_clients_docs);
                docs.extend(ingest_pipelines_docs);
                docs.extend(recording_docs);
                docs.extend(transport_actions_docs);
                log::trace!("node_stats docs for {}: {}", node_id, docs.len());
                docs
            })
            .collect();

        log::debug!("node_stats docs: {}", node_stats_docs.len());
        (data_stream, node_stats_docs)
    }
}
