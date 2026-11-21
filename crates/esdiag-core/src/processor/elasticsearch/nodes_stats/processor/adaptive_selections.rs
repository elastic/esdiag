// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{Lookup, metadata::MetadataRawValue, nodes::NodeDocument};
use eyre::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc::Sender;

/// Extract adaptive_selection
pub async fn extract(
    sender: &Sender<AdaptiveSelectionDoc>,
    adaptive_selection: Option<Value>,
    metadata: &MetadataRawValue,
    node_metadata: Option<&NodeDocument>,
    lookup_node: &Lookup<NodeDocument>,
) -> Result<()> {
    let adaptive_selection = match adaptive_selection {
        Some(Value::Object(data)) => data,
        _ => return Err(eyre::eyre!("Error extracting node.adaptive_selection data")),
    };

    let mut docs = Vec::<AdaptiveSelectionDoc>::with_capacity(200);
    docs.extend(
        adaptive_selection
            .into_iter()
            .map(|(peer_node_id, adaptive_selection)| AdaptiveSelectionDoc {
                node: node_metadata.cloned(),
                adaptive_selection: AdaptiveSelectionEntry {
                    node: lookup_node.by_id(&peer_node_id).cloned(),
                    data: adaptive_selection,
                },
                metadata: metadata.clone(),
            }),
    );

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct AdaptiveSelectionDoc {
    node: Option<NodeDocument>,
    adaptive_selection: AdaptiveSelectionEntry,
    #[serde(flatten)]
    metadata: MetadataRawValue,
}

#[derive(Serialize)]
struct AdaptiveSelectionEntry {
    node: Option<NodeDocument>,
    #[serde(flatten)]
    data: Value,
}
