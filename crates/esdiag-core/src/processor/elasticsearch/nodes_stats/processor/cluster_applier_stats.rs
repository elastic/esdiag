// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{metadata::MetadataRawValue, nodes::NodeDocument};
use eyre::{OptionExt, Result};
use serde::Serialize;
use serde_json::{Value, value::RawValue};
use tokio::sync::mpsc::Sender;

/// Extract discovery.cluster_applier_stats.recordings dataset
pub async fn extract(
    sender: &Sender<ClusterApplierDoc>,
    mut cluster_applier_stats: Value,
    metadata: &MetadataRawValue,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let recordings = cluster_applier_stats["recordings"]
        .as_array_mut()
        .ok_or_eyre("Error extracting node.discovery.cluster_applier data")?;

    let mut docs = Vec::<ClusterApplierDoc>::with_capacity(200);
    docs.extend(recordings.drain(..).filter_map(|recording| {
        let recording_raw = match serde_json::value::to_raw_value(&recording) {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!("Skipping malformed cluster applier recording: {}", err);
                return None;
            }
        };
        Some(ClusterApplierDoc {
            cluster_applier_stats: recording_raw,
            node: node_metadata.cloned(),
            metadata: metadata.clone(),
        })
    }));

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ClusterApplierDoc {
    cluster_applier_stats: Box<RawValue>,
    node: Option<NodeDocument>,
    #[serde(flatten)]
    metadata: MetadataRawValue,
}
