// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

mod adaptive_selections;
mod cluster_applier_stats;
mod http_clients;
mod ingest_pipelines;
mod transport_actions;

use super::super::super::{Exporter, ProcessorSummary};
use super::super::{
    DocumentExporter, ElasticsearchMetadata, Lookups, SharedCacheStats, metadata::MetadataRawValue,
};
use super::NodesStats;
use crate::processor::StreamingDocumentExporter;
use futures::stream::{BoxStream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::sync::LazyLock;
use tokio::sync::mpsc;

static INGEST_ROLE: LazyLock<String> = LazyLock::new(|| String::from("ingest"));

struct NodeProcessingContext<'a> {
    lookups: &'a Lookups,
    metadata: &'a ElasticsearchMetadata,
    node_stats_metadata: &'a MetadataRawValue,
    actions_metadata: &'a MetadataRawValue,
    http_clients_metadata: &'a MetadataRawValue,
    applier_metadata: &'a MetadataRawValue,
    adaptive_metadata: &'a MetadataRawValue,
    nodes_stats_tx: &'a mpsc::Sender<NodeStatsDoc>,
    actions_tx: &'a mpsc::Sender<transport_actions::TransportActionDoc>,
    http_clients_tx: &'a mpsc::Sender<http_clients::HttpClientDoc>,
    applier_tx: &'a mpsc::Sender<cluster_applier_stats::ClusterApplierDoc>,
    adaptive_tx: &'a mpsc::Sender<adaptive_selections::AdaptiveSelectionDoc>,
    pipelines_tx: &'a mpsc::Sender<ingest_pipelines::IngestPipelineDoc>,
    processors_tx: &'a mpsc::Sender<ingest_pipelines::IngestDoc>,
}

async fn process_node(
    node_id: String,
    mut node_stats: super::data::NodeStats,
    ctx: &NodeProcessingContext<'_>,
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
            let actions = transport_val
                .as_object_mut()
                .and_then(|obj| obj.remove("actions"))
                .unwrap_or(Value::Null);
            let mut extracted_actions = true;
            if !actions.is_null()
                && let Err(e) = transport_actions::extract(
                    ctx.actions_tx,
                    actions,
                    ctx.actions_metadata,
                    node_metadata,
                )
                .await
            {
                extracted_actions = false;
                tracing::error!(
                    "Error extracting transport stats for node {}: {}",
                    node_id,
                    e
                );
            }
            if extracted_actions {
                match serde_json::value::RawValue::from_string(transport_val.to_string()) {
                    Ok(raw) => node_stats.transport = Some(raw),
                    Err(e) => {
                        tracing::error!("Failed to re-serialize transport stats: {}", e);
                        // Trade-off: mutating RawValue requires serialization cycle. If it fails, fallback to un-mutated.
                        // This means transport.actions remain in the doc, leading to potentially larger payload.
                        node_stats.transport = Some(transport_raw);
                    }
                }
            } else {
                node_stats.transport = Some(transport_raw);
            }
        } else {
            node_stats.transport = Some(transport_raw);
        }
    }

    // Extract HTTP clients
    if let Ok(mut http_val) = serde_json::from_str::<Value>(node_stats.http.get()) {
        let clients = http_val
            .as_object_mut()
            .and_then(|obj| obj.remove("clients"))
            .unwrap_or(Value::Null);
        if let Some(obj) = http_val.as_object_mut() {
            obj.remove("routes");
        }
        let mut extracted_clients = true;
        if !clients.is_null()
            && let Err(e) = http_clients::extract(
                ctx.http_clients_tx,
                clients,
                ctx.http_clients_metadata,
                node_metadata,
            )
            .await
        {
            extracted_clients = false;
            tracing::error!("Error extracting HTTP clients stats: {}", e);
        }
        if extracted_clients {
            match serde_json::value::RawValue::from_string(http_val.to_string()) {
                Ok(raw) => node_stats.http = raw,
                Err(e) => {
                    tracing::error!("Failed to re-serialize HTTP clients stats: {}", e);
                }
            }
        }
    }

    // Extract adaptive replica selection stats
    if let Some(adaptive_raw) = node_stats.adaptive_selection.take()
        && let Ok(adaptive_val) = serde_json::from_str::<Value>(adaptive_raw.get())
        && let Err(e) = adaptive_selections::extract(
            ctx.adaptive_tx,
            Some(adaptive_val),
            ctx.adaptive_metadata,
            node_metadata,
            lookup_node,
        )
        .await
    {
        tracing::error!("Error extracting adaptive selection stats: {}", e);
    }

    // Extract cluster applier state
    if let Ok(mut discovery_val) = serde_json::from_str::<Value>(node_stats.discovery.get()) {
        let cluster_applier_stats = discovery_val
            .as_object_mut()
            .and_then(|obj| obj.remove("cluster_applier_stats"))
            .unwrap_or(Value::Null);
        if !cluster_applier_stats.is_null() {
            let extract_result = cluster_applier_stats::extract(
                ctx.applier_tx,
                cluster_applier_stats,
                ctx.applier_metadata,
                node_metadata,
            )
            .await;
            if let Err(e) = extract_result {
                tracing::error!("Error extracting cluster applier stats: {}", e);
            } else {
                match serde_json::value::RawValue::from_string(discovery_val.to_string()) {
                    Ok(raw) => node_stats.discovery = raw,
                    Err(e) => {
                        tracing::error!("Failed to re-serialize cluster applier stats: {}", e);
                    }
                }
            }
        }
    }

    // Extract ingest pipeline stats, but only on nodes with the `ingest` role
    if node_stats.roles.contains(&*INGEST_ROLE)
        && let Err(e) = ingest_pipelines::extract(
            ctx.pipelines_tx,
            ctx.processors_tx,
            node_stats.ingest.pipelines.take(),
            ctx.metadata,
            node_metadata,
        )
        .await
    {
        tracing::error!("Error extracting ingest pipelines stats: {}", e);
    }

    // Final node_stats document
    let doc = NodeStatsDoc {
        node: NodeStatsEnvelope {
            stats: node_stats,
            id: node_metadata.as_ref().and_then(|node| node.id.clone()),
            role: node_metadata.as_ref().map(|node| node.role.clone()),
            tier: node_metadata.as_ref().map(|node| node.tier.clone()),
            tier_order: node_metadata.as_ref().map(|node| node.tier_order),
        },
        shared_cache: lookup_shared_cache.by_id(node_id.as_str()).cloned(),
        metadata: ctx.node_stats_metadata.clone(),
    };

    if (ctx.nodes_stats_tx.send(doc).await).is_err() {
        tracing::warn!("Nodes stats channel closed unexpectedly");
    }
}

impl DocumentExporter<Lookups, ElasticsearchMetadata> for NodesStats {
    async fn documents_export(
        self,
        exporter: &Exporter,
        lookups: &Lookups,
        metadata: &ElasticsearchMetadata,
    ) -> ProcessorSummary {
        tracing::debug!("nodes: {}", self.nodes.len());
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
        tracing::debug!("Processing node_stats stream");
        let data_stream = "metrics-node-esdiag".to_string();
        let mut summary = ProcessorSummary::new(data_stream.clone());

        let batch_size = 5000;
        const BUFFER_SIZE: usize = 5000;

        let (nodes_stats_tx, nodes_stats_rx) = mpsc::channel::<NodeStatsDoc>(BUFFER_SIZE);
        let node_stats_metadata = metadata.for_data_stream(&data_stream);
        let nodes_stats_processor =
            tokio::spawn(exporter.clone().document_channel::<NodeStatsDoc>(
                nodes_stats_rx,
                "metrics-node-esdiag".to_string(),
                batch_size,
            ));

        let (actions_tx, actions_rx) =
            mpsc::channel::<transport_actions::TransportActionDoc>(BUFFER_SIZE);
        let actions_data_stream = "metrics-node.transport.actions-esdiag".to_string();
        let actions_metadata = metadata.for_data_stream(&actions_data_stream);
        let actions_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<transport_actions::TransportActionDoc>(
                    actions_rx,
                    actions_data_stream,
                    batch_size,
                ),
        );

        let (http_clients_tx, http_clients_rx) =
            mpsc::channel::<http_clients::HttpClientDoc>(BUFFER_SIZE);
        let http_clients_data_stream = "metrics-node.http.clients-esdiag".to_string();
        let http_clients_metadata = metadata.for_data_stream(&http_clients_data_stream);
        let http_clients_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<http_clients::HttpClientDoc>(
                    http_clients_rx,
                    http_clients_data_stream,
                    batch_size,
                ),
        );

        let (applier_tx, applier_rx) =
            mpsc::channel::<cluster_applier_stats::ClusterApplierDoc>(BUFFER_SIZE);
        let applier_data_stream = "metrics-node.discovery.cluster_applier-esdiag".to_string();
        let applier_metadata = metadata.for_data_stream(&applier_data_stream);
        let applier_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<cluster_applier_stats::ClusterApplierDoc>(
                    applier_rx,
                    applier_data_stream,
                    batch_size,
                ),
        );

        let (adaptive_tx, adaptive_rx) =
            mpsc::channel::<adaptive_selections::AdaptiveSelectionDoc>(BUFFER_SIZE);
        let adaptive_data_stream = "metrics-node.discovery.cluster_adaptive-esdiag".to_string();
        let adaptive_metadata = metadata.for_data_stream(&adaptive_data_stream);
        let adaptive_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<adaptive_selections::AdaptiveSelectionDoc>(
                    adaptive_rx,
                    adaptive_data_stream,
                    batch_size,
                ),
        );

        let (pipelines_tx, pipelines_rx) =
            mpsc::channel::<ingest_pipelines::IngestPipelineDoc>(BUFFER_SIZE);
        let pipelines_data_stream = "metrics-ingest.pipeline-esdiag".to_string();
        let pipelines_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<ingest_pipelines::IngestPipelineDoc>(
                    pipelines_rx,
                    pipelines_data_stream,
                    batch_size,
                ),
        );

        let (processors_tx, processors_rx) =
            mpsc::channel::<ingest_pipelines::IngestDoc>(BUFFER_SIZE);
        let processors_data_stream = "metrics-ingest.processors-esdiag".to_string();
        let processors_processor = tokio::spawn(
            exporter
                .clone()
                .document_channel::<ingest_pipelines::IngestDoc>(
                    processors_rx,
                    processors_data_stream,
                    batch_size,
                ),
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok((node_id, node_stats)) => {
                    let ctx = NodeProcessingContext {
                        lookups,
                        metadata,
                        node_stats_metadata: &node_stats_metadata,
                        actions_metadata: &actions_metadata,
                        http_clients_metadata: &http_clients_metadata,
                        applier_metadata: &applier_metadata,
                        adaptive_metadata: &adaptive_metadata,
                        nodes_stats_tx: &nodes_stats_tx,
                        actions_tx: &actions_tx,
                        http_clients_tx: &http_clients_tx,
                        applier_tx: &applier_tx,
                        adaptive_tx: &adaptive_tx,
                        pipelines_tx: &pipelines_tx,
                        processors_tx: &processors_tx,
                    };
                    process_node(node_id, node_stats, &ctx).await;
                }
                Err(e) => {
                    tracing::error!("Error reading from node stats stream: {}", e);
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

#[derive(Serialize)]
struct NodeStatsDoc {
    node: NodeStatsEnvelope,
    shared_cache: Option<SharedCacheStats>,
    #[serde(flatten)]
    metadata: MetadataRawValue,
}

#[derive(Serialize)]
struct NodeStatsEnvelope {
    #[serde(flatten)]
    stats: super::data::NodeStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier_order: Option<usize>,
}
