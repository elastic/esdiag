// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod adaptive_selections;
mod cluster_applier_stats;
mod http_clients;
mod ingest_pipelines;
mod transport_actions;

use super::super::super::{Exporter, ProcessorSummary};
use super::super::{DocumentExporter, ElasticsearchMetadata, Lookups, metadata::PreSerializedMetadata};
use super::NodesStats;
use crate::processor::StreamingDocumentExporter;
use futures::stream::{BoxStream, StreamExt};
use json_patch::merge;
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::mpsc;

static INGEST_ROLE: LazyLock<String> = LazyLock::new(|| String::from("ingest"));

struct NodeProcessingContext {
    lookups: Lookups,
    metadata: ElasticsearchMetadata,
    node_stats_metadata: PreSerializedMetadata,
    actions_metadata: PreSerializedMetadata,
    http_clients_metadata: PreSerializedMetadata,
    applier_metadata: PreSerializedMetadata,
    adaptive_metadata: PreSerializedMetadata,
    nodes_stats_tx: mpsc::Sender<Value>,
    actions_tx: mpsc::Sender<Value>,
    http_clients_tx: mpsc::Sender<Value>,
    applier_tx: mpsc::Sender<Value>,
    adaptive_tx: mpsc::Sender<Value>,
    pipelines_tx: mpsc::Sender<Value>,
    processors_tx: mpsc::Sender<Value>,
}

async fn process_node(
    node_id: String,
    mut node_stats: super::data::NodeStats,
    ctx: &NodeProcessingContext,
) {
    let lookup_node = &ctx.lookups.node;
    let lookup_shared_cache = &ctx.lookups.shared_cache;

    let node_metadata = lookup_node.by_id(&node_id);
    let allocated_processors = node_metadata
        .map(|node| node.os.allocated_processors)
        .unwrap_or(1);
    node_stats.calculate_stats(allocated_processors);

    // Extract transport actions
    if let Some(transport_raw) = node_stats.transport.take() {
        if let Ok(mut transport_val) = serde_json::from_str::<Value>(transport_raw.get()) {
            let actions = transport_val["actions"].take();
            if !actions.is_null() {
                // Since actions_metadata is pre-serialized, we need a Value for merge in transport_actions::extract
                // For now, transport_actions::extract still expects &Value.
                // We'll convert it back if needed or update the helper.
                let actions_meta_val = serde_json::to_value(&ctx.actions_metadata).unwrap();
                if let Err(e) = transport_actions::extract(
                    &ctx.actions_tx,
                    actions,
                    &actions_meta_val,
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
            node_stats.transport =
                serde_json::value::RawValue::from_string(transport_val.to_string()).ok();
        } else {
            node_stats.transport = Some(transport_raw);
        }
    }

    // Extract HTTP clients
    if let Ok(mut http_val) = serde_json::from_str::<Value>(node_stats.http.get()) {
        let clients = http_val["clients"].take();
        if !clients.is_null() {
            let http_meta_val = serde_json::to_value(&ctx.http_clients_metadata).unwrap();
            if let Err(e) = http_clients::extract(
                &ctx.http_clients_tx,
                clients,
                &http_meta_val,
                node_metadata,
            )
            .await
            {
                log::error!("Error extracting HTTP clients stats: {}", e);
            }
            if let Ok(raw) = serde_json::value::RawValue::from_string(http_val.to_string()) {
                node_stats.http = raw;
            }
        }
    }

    // Extract adaptive replica selection stats
    if let Some(adaptive_raw) = node_stats.adaptive_selection.take() {
        if let Ok(adaptive_val) = serde_json::from_str::<Value>(adaptive_raw.get()) {
            let adaptive_meta_val = serde_json::to_value(&ctx.adaptive_metadata).unwrap();
            if let Err(e) = adaptive_selections::extract(
                &ctx.adaptive_tx,
                Some(adaptive_val),
                &adaptive_meta_val,
                node_metadata,
                lookup_node,
            )
            .await
            {
                log::error!("Error extracting adaptive selection stats: {}", e);
            }
        }
    }

    // Extract cluster applier state
    if let Ok(mut discovery_val) = serde_json::from_str::<Value>(node_stats.discovery.get()) {
        let cluster_applier_stats = discovery_val["cluster_applier_stats"].take();
        if !cluster_applier_stats.is_null() {
            let applier_meta_val = serde_json::to_value(&ctx.applier_metadata).unwrap();
            if let Err(e) = cluster_applier_stats::extract(
                &ctx.applier_tx,
                cluster_applier_stats,
                &applier_meta_val,
                node_metadata,
            )
            .await
            {
                log::error!("Error extracting cluster applier stats: {}", e);
            }
            if let Ok(raw) = serde_json::value::RawValue::from_string(discovery_val.to_string()) {
                node_stats.discovery = raw;
            }
        }
    }

    // Extract ingest pipeline stats, but only on nodes with the `ingest` role
    if node_stats.roles.contains(&*INGEST_ROLE) {
        if let Err(e) = ingest_pipelines::extract(
            &ctx.pipelines_tx,
            &ctx.processors_tx,
            node_stats.ingest.pipelines.take(),
            ctx.metadata.clone(),
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
    let node_stats_meta_val = serde_json::to_value(&ctx.node_stats_metadata).unwrap();

    merge(&mut doc, &node_stats_meta_val);
    merge(&mut doc, &node_summary_patch);
    merge(&mut doc, &omit_patch);

    if (ctx.nodes_stats_tx.send(doc).await).is_err() {
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

        let batch_size = 5000;
        const BUFFER_SIZE: usize = 5000;

        let (nodes_stats_tx, nodes_stats_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let node_stats_metadata = metadata.for_data_stream(&data_stream).pre_serialize();
        let nodes_stats_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            nodes_stats_rx,
            "metrics-node-esdiag".to_string(),
            batch_size,
        ));

        let (actions_tx, actions_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let actions_data_stream = "metrics-node.transport.actions-esdiag".to_string();
        let actions_metadata = metadata.for_data_stream(&actions_data_stream).pre_serialize();
        let actions_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            actions_rx,
            actions_data_stream,
            batch_size,
        ));

        let (http_clients_tx, http_clients_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let http_clients_data_stream = "metrics-node.http.clients-esdiag".to_string();
        let http_clients_metadata = metadata
            .for_data_stream(&http_clients_data_stream)
            .pre_serialize();
        let http_clients_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            http_clients_rx,
            http_clients_data_stream,
            batch_size,
        ));

        let (applier_tx, applier_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let applier_data_stream = "metrics-node.discovery.cluster_applier-esdiag".to_string();
        let applier_metadata = metadata.for_data_stream(&applier_data_stream).pre_serialize();
        let applier_processor = tokio::spawn(exporter.clone().document_channel::<Value>(
            applier_rx,
            applier_data_stream,
            batch_size,
        ));

        let (adaptive_tx, adaptive_rx) = mpsc::channel::<Value>(BUFFER_SIZE);
        let adaptive_data_stream = "metrics-node.discovery.cluster_adaptive-esdiag".to_string();
        let adaptive_metadata = metadata
            .for_data_stream(&adaptive_data_stream)
            .pre_serialize();
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
                    let ctx = NodeProcessingContext {
                        lookups: lookups.clone(),
                        metadata: metadata.clone(),
                        node_stats_metadata: node_stats_metadata.clone(),
                        actions_metadata: actions_metadata.clone(),
                        http_clients_metadata: http_clients_metadata.clone(),
                        applier_metadata: applier_metadata.clone(),
                        adaptive_metadata: adaptive_metadata.clone(),
                        nodes_stats_tx: nodes_stats_tx.clone(),
                        actions_tx: actions_tx.clone(),
                        http_clients_tx: http_clients_tx.clone(),
                        applier_tx: applier_tx.clone(),
                        adaptive_tx: adaptive_tx.clone(),
                        pipelines_tx: pipelines_tx.clone(),
                        processors_tx: processors_tx.clone(),
                    };
                    process_node(node_id, node_stats, &ctx).await;
                }
                Err(e) => {
                    log::error!("Error reading from node stats stream: {}", e);
                }
            }
        }

        // Close channels
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

        summary
    }
}
