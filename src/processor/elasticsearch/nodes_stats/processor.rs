// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod adaptive_selections;
mod cluster_applier_stats;
mod http_clients;
mod ingest_pipelines;
mod transport_actions;

use super::super::super::{Exporter, ProcessorSummary};
use crate::processor::StreamingDocumentExporter;
use super::super::{
    DocumentExporter, ElasticsearchMetadata, Lookups, Metadata,
};
use super::NodesStats;
use futures::stream::{BoxStream, StreamExt};
use json_patch::merge;
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::mpsc;

static INGEST_ROLE: LazyLock<String> = LazyLock::new(|| String::from("ingest"));

async fn process_node(
    node_id: String,
    mut node_stats: super::data::NodeStats,
    lookups: &Lookups,
    metadata: &ElasticsearchMetadata,
    node_stats_metadata: &Value,
    actions_metadata: &Value,
    http_clients_metadata: &Value,
    applier_metadata: &Value,
    adaptive_metadata: &Value,
    nodes_stats_tx: &mpsc::Sender<Value>,
    actions_tx: &mpsc::Sender<Value>,
    http_clients_tx: &mpsc::Sender<Value>,
    applier_tx: &mpsc::Sender<Value>,
    adaptive_tx: &mpsc::Sender<Value>,
    pipelines_tx: &mpsc::Sender<Value>,
    processors_tx: &mpsc::Sender<Value>,
) {
    let lookup_node = &lookups.node;
    let lookup_shared_cache = &lookups.shared_cache;

    let node_metadata = lookup_node.by_id(&node_id);
    let allocated_processors = node_metadata
        .map(|node| node.os.allocated_processors)
        .unwrap_or(1);
    node_stats.calculate_stats(allocated_processors);

    // Extract transport actions
    if let Some(ref mut transport) = node_stats.transport {
        let actions = transport["actions"].take();
        if !actions.is_null() {
            if let Err(e) = transport_actions::extract(
                actions_tx,
                actions,
                actions_metadata,
                node_metadata,
            )
            .await
            {
                log::error!(
                    "Error extracting transport stats for node {}: {}",
                    node_id,
                    e
                );
            }
        }
    }

    // Extract HTTP clients
    if let Err(e) = http_clients::extract(
        http_clients_tx,
        node_stats.http["clients"].take(),
        http_clients_metadata,
        node_metadata,
    )
    .await
    {
        log::error!("Error extracting HTTP clients stats: {}", e);
    }

    // Extract adaptive replica selection stats
    if let Err(e) = adaptive_selections::extract(
        adaptive_tx,
        node_stats.adaptive_selection.take(),
        adaptive_metadata,
        node_metadata,
        lookup_node,
    )
    .await
    {
        log::error!("Error extracting adaptive selection stats: {}", e);
    }

    // Extract cluster applier state
    if let Err(e) = cluster_applier_stats::extract(
        applier_tx,
        node_stats.discovery["cluster_applier_stats"].take(),
        applier_metadata,
        node_metadata,
    )
    .await
    {
        log::error!("Error extracting cluster applier stats: {}", e);
    }

    // Extract ingest pipeline stats, but only on nodes with the `ingest` role
    if node_stats.roles.contains(&*INGEST_ROLE) {
        if let Err(e) = ingest_pipelines::extract(
            pipelines_tx,
            processors_tx,
            node_stats.ingest.pipelines.take(),
            metadata.clone(),
            node_metadata,
        )
        .await
        {
            log::error!("Error extracting ingest pipelines stats: {}", e);
        }
    }

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

    let node_summary_patch = json!({"node": node_metadata});

    merge(&mut doc, node_stats_metadata);
    merge(&mut doc, &node_summary_patch);
    merge(&mut doc, &omit_patch);

    if nodes_stats_tx.send(doc).await.is_err() {
        log::warn!("Nodes stats channel closed unexpectedly");
    }
}

impl DocumentExporter<Lookups, ElasticsearchMetadata> for NodesStats {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("nodes: {}", self.nodes.len());
        let stream = futures::stream::iter(self.nodes.into_iter().map(Ok));
        Self::documents_export_stream(Box::pin(stream), exporter, lookups, metadata).await
    }
}

impl StreamingDocumentExporter<Lookups, ElasticsearchMetadata> for NodesStats {
    async fn documents_export_stream(
        mut stream: BoxStream<'static, Result<Self::Item, eyre::Report>>,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        log::debug!("Processing node_stats stream");
        let data_stream = "metrics-node-esdiag".to_string();
        let mut summary = ProcessorSummary::new(data_stream.clone());

        // Tune batch sizes and channel buffers for memory usage and write frequency
        let batch_size = 5000;
        const BUFFER_SIZE: usize = 5000;

        // Spawn document channels for concurrent processing with backpressure
        let (nodes_stats_tx, nodes_stats_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let nodes_stats_data_stream = "metrics-node-esdiag".to_string();
        let node_stats_metadata = metadata.for_data_stream(&data_stream).as_meta_doc();
        let nodes_stats_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            nodes_stats_rx,
            nodes_stats_data_stream,
            batch_size,
        ));

        let (actions_tx, actions_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let actions_data_stream = "metrics-node.transport.actions-esdiag".to_string();
        let actions_metadata = metadata.for_data_stream(&actions_data_stream).as_meta_doc();
        let actions_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            actions_rx,
            actions_data_stream,
            batch_size,
        ));

        let (http_clients_tx, http_clients_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let http_clients_data_stream = "metrics-node.http.clients-esdiag".to_string();
        let http_clients_metadata = metadata
            .for_data_stream(&http_clients_data_stream)
            .as_meta_doc();
        let http_clients_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            http_clients_rx,
            http_clients_data_stream,
            batch_size,
        ));

        let (applier_tx, applier_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let applier_data_stream = "metrics-node.discovery.cluster_applier-esdiag".to_string();
        let applier_metadata = metadata.for_data_stream(&applier_data_stream).as_meta_doc();
        let applier_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            applier_rx,
            applier_data_stream,
            batch_size,
        ));

        let (adaptive_tx, adaptive_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let adaptive_data_stream = "metrics-node.discovery.cluster_adaptive-esdiag".to_string();
        let adaptive_metadata = metadata
            .for_data_stream(&adaptive_data_stream)
            .as_meta_doc();
        let adaptive_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            adaptive_rx,
            adaptive_data_stream,
            batch_size,
        ));

        let (pipelines_tx, pipelines_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let pipelines_data_stream = "metrics-ingest.pipeline-esdiag".to_string();
        let pipelines_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            pipelines_rx,
            pipelines_data_stream,
            batch_size,
        ));

        let (processors_tx, processors_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let processors_data_stream = "metrics-ingest.processors-esdiag".to_string();
        let processors_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            processors_rx,
            processors_data_stream,
            batch_size,
        ));

        while let Some(result) = stream.next().await {
            match result {
                Ok((node_id, node_stats)) => {
                    process_node(
                        node_id,
                        node_stats,
                        lookups,
                        metadata,
                        &node_stats_metadata,
                        &actions_metadata,
                        &http_clients_metadata,
                        &applier_metadata,
                        &adaptive_metadata,
                        &nodes_stats_tx,
                        &actions_tx,
                        &http_clients_tx,
                        &applier_tx,
                        &adaptive_tx,
                        &pipelines_tx,
                        &processors_tx,
                    )
                    .await;
                }
                Err(e) => {
                    log::error!("Error reading from node stats stream: {}", e);
                }
            }
        }

        // Close channels to signal completion
        drop(nodes_stats_tx);
        drop(actions_tx);
        drop(http_clients_tx);
        drop(applier_tx);
        drop(adaptive_tx);
        drop(pipelines_tx);
        drop(processors_tx);

        let (
            nodes_stats_result,
            actions_result,
            http_clients_result,
            applier_result,
            adaptive_result,
            pipelines_result,
            processors_result,
        ) = tokio::join!(
            nodes_stats_processor,
            actions_processor,
            http_clients_processor,
            applier_processor,
            adaptive_processor,
            pipelines_processor,
            processors_processor
        );

        summary.merge(nodes_stats_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(actions_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(http_clients_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(applier_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(adaptive_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(pipelines_result.map_err(|err| eyre::Report::new(err)));
        summary.merge(processors_result.map_err(|err| eyre::Report::new(err)));

        log::debug!("node_stats stream processed: {}", summary.docs);
        summary
    }
}
