// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{metadata::MetadataRawValue, nodes::NodeDocument};
use eyre::{OptionExt, Result};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc::Sender;

/// Extract http.clients
pub async fn extract(
    sender: &Sender<HttpClientDoc>,
    mut clients: Value,
    metadata: &MetadataRawValue,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let clients = clients
        .as_array_mut()
        .ok_or_eyre("Error extracting node.http.clients data")?;

    let mut docs = Vec::<HttpClientDoc>::with_capacity(200);
    docs.par_extend(clients.par_drain(..).map(|client| {
        HttpClientDoc {
            node: node_metadata.cloned(),
            http: HttpClientContainer { client },
            metadata: metadata.clone(),
        }
    }));

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct HttpClientDoc {
    node: Option<NodeDocument>,
    http: HttpClientContainer,
    #[serde(flatten)]
    metadata: MetadataRawValue,
}

#[derive(Serialize)]
struct HttpClientContainer {
    client: Value,
}
