// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{Lookup, metadata::MetadataRawValue, nodes::NodeDocument};
use eyre::Result;
use serde::{Serialize, Serializer};
use serde_json::{Value, value::RawValue};
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
            .collect::<Vec<(String, Value)>>()
            .drain(..)
            .map(|(peer_node_id, adaptive_selection)| {
                AdaptiveSelectionDoc {
                    node: node_metadata.cloned(),
                    adaptive_selection: AdaptiveSelectionEntry {
                        node: lookup_node.by_id(&peer_node_id).cloned(),
                        data: FlattenRawValue(
                            serde_json::value::to_raw_value(&adaptive_selection)
                                .expect("serialize adaptive selection to raw"),
                        ),
                    },
                    metadata: metadata.clone(),
                }
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
    data: FlattenRawValue,
}

struct FlattenRawValue(Box<RawValue>);

impl Serialize for FlattenRawValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value: Value =
            serde_json::from_str(self.0.get()).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}
