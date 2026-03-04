// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::nodes::NodeDocument;
use eyre::{OptionExt, Result};
use json_patch::merge;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

/// Extract discovery.cluster_applier_stats.recordings dataset
pub async fn extract(
    sender: &Sender<Value>,
    mut cluster_applier_stats: Value,
    metadata: &Value,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let recordings = cluster_applier_stats["recordings"]
        .as_array_mut()
        .ok_or_eyre("Error extracting node.discovery.cluster_applier data")?;

    let mut docs = Vec::<Value>::with_capacity(200);
    docs.extend(recordings.drain(..).map(|recording| {
        let mut doc = json!({
            "cluster_applier_stats": recording,
            "node": node_metadata,
        });

        merge(&mut doc, metadata);
        doc
    }));

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}
