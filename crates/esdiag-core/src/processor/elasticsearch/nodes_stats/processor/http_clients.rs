// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{metadata::MetadataValue, nodes::NodeDocument};
use eyre::{OptionExt, Result};
use serde::Serialize;
use serde_json::{Value, value::RawValue};
use tokio::sync::mpsc::Sender;

/// Extract http.clients
pub async fn extract(
    sender: &Sender<HttpClientDoc>,
    mut clients: Value,
    metadata: &MetadataValue,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let clients = clients
        .as_array_mut()
        .ok_or_eyre("Error extracting node.http.clients data")?;

    for client in clients.drain(..) {
        let client_raw = match serde_json::value::to_raw_value(&client) {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!("Skipping malformed http client stats record: {}", err);
                continue;
            }
        };
        sender
            .send(HttpClientDoc {
                node: node_metadata.cloned(),
                http: HttpClientContainer { client: client_raw },
                metadata: metadata.clone(),
            })
            .await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct HttpClientDoc {
    node: Option<NodeDocument>,
    http: HttpClientContainer,
    #[serde(flatten)]
    metadata: MetadataValue,
}

#[derive(Serialize)]
struct HttpClientContainer {
    client: Box<RawValue>,
}
