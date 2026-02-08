// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::nodes::NodeDocument;
use eyre::{OptionExt, Result};
use json_patch::merge;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

/// Extract transport.actions
pub async fn extract(
    sender: &Sender<Value>,
    mut actions: Value,
    metadata: &Value,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let actions = actions
        .as_object_mut()
        .ok_or_eyre("Error extracting node transport.actions data")?;

    let mut docs = Vec::<Value>::with_capacity(100);
    docs.extend(
        actions
            .into_iter()
            .collect::<Vec<_>>()
            .drain(..)
            .map(|(name, action)| {
                let mut action = json!({
                    "node": node_metadata,
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
                merge(&mut action, &metadata);
                action
            }),
    );

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}
