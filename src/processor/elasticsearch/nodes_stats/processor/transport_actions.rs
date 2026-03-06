// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::{metadata::MetadataRawValue, nodes::NodeDocument};
use eyre::{OptionExt, Result};
use serde::{Serialize, Serializer};
use serde_json::{Value, value::RawValue};
use tokio::sync::mpsc::Sender;

/// Extract transport.actions
pub async fn extract(
    sender: &Sender<TransportActionDoc>,
    mut actions: Value,
    metadata: &MetadataRawValue,
    node_metadata: Option<&NodeDocument>,
) -> Result<()> {
    let actions = actions
        .as_object_mut()
        .ok_or_eyre("Error extracting node transport.actions data")?;

    let mut docs = Vec::<TransportActionDoc>::with_capacity(100);
    docs.extend(
        std::mem::take(actions)
            .into_iter()
            .map(|(name, action)| {
                TransportActionDoc {
                    node: node_metadata.cloned(),
                    metadata: metadata.clone(),
                    transport: TransportActionContainer {
                        action: NamedAction {
                            name,
                            data: FlattenRawValue(
                                serde_json::value::to_raw_value(&action)
                                    .expect("serialize transport action to raw"),
                            ),
                        },
                    },
                }
            }),
    );

    for doc in docs {
        sender.send(doc).await?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct TransportActionDoc {
    node: Option<NodeDocument>,
    transport: TransportActionContainer,
    #[serde(flatten)]
    metadata: MetadataRawValue,
}

#[derive(Serialize)]
struct TransportActionContainer {
    action: NamedAction,
}

#[derive(Serialize)]
struct NamedAction {
    name: String,
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
