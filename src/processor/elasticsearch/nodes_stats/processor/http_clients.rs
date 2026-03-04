// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::nodes::NodeDocument;
use eyre::{OptionExt, Result};
use json_patch::merge;
use rayon::prelude::*;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

/// Extract http.clients
pub async fn extract(
    sender: &Sender<Value>,
    mut clients: Value,
    metadata: &Value,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let clients = clients
        .as_array_mut()
        .ok_or_eyre("Error extracting node.http.clients data")?;

    let mut docs = Vec::<Value>::with_capacity(200);
    docs.par_extend(clients.par_drain(..).map(|client| {
        let mut doc = json!({ "node": node_metadata, "http": { "client": client }});
        merge(&mut doc, metadata);
        doc
    }));

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}
