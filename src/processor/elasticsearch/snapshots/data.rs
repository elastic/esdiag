// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::super::diagnostic::data_source::StreamingDataSource;
use super::super::DataSource;
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

pub type Repositories = HashMap<String, RepositoryConfig>;

#[derive(Clone, Deserialize, Serialize)]
pub struct RepositoryConfig {
    #[serde(rename = "type")]
    pub repository_type: Option<String>,
    pub settings: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Snapshots {
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Snapshot {
    pub snapshot: String,
    pub repository: Option<String>,
    pub state: Option<String>,
    pub indices: Option<Box<RawValue>>,
    pub data_streams: Option<Box<RawValue>>,
    pub start_time: Option<String>,
    pub start_time_in_millis: Option<u64>,
    pub end_time: Option<String>,
    pub end_time_in_millis: Option<u64>,
    pub duration_in_millis: Option<u64>,
}

pub fn extract_snapshot_date(snapshot_name: &str) -> Option<String> {
    let bytes = snapshot_name.as_bytes();
    if bytes.len() < 10 {
        return None;
    }

    for window in bytes.windows(10) {
        let valid = window[0].is_ascii_digit()
            && window[1].is_ascii_digit()
            && window[2].is_ascii_digit()
            && window[3].is_ascii_digit()
            && window[4] == b'.'
            && window[5].is_ascii_digit()
            && window[6].is_ascii_digit()
            && window[7] == b'.'
            && window[8].is_ascii_digit()
            && window[9].is_ascii_digit();
        if valid {
            let year = std::str::from_utf8(&window[0..4]).ok()?;
            let month = std::str::from_utf8(&window[5..7]).ok()?;
            let day = std::str::from_utf8(&window[8..10]).ok()?;
            let month_num = month.parse::<u32>().ok()?;
            let day_num = day.parse::<u32>().ok()?;
            if !(1..=12).contains(&month_num) || !(1..=31).contains(&day_num) {
                continue;
            }
            return Some(format!("{year}-{month}-{day}"));
        }
    }
    None
}

impl StreamingDataSource for Snapshots {
    type Item = Snapshot;

    fn deserialize_stream<'de, D>(
        deserializer: D,
        sender: Sender<Result<Self::Item>>,
    ) -> std::result::Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SnapshotRootVisitor {
            sender: Sender<Result<Snapshot>>,
        }

        impl<'de> serde::de::Visitor<'de> for SnapshotRootVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("snapshot root object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<String>()? {
                    if key == "snapshots" {
                        map.next_value_seed(SnapshotSeqVisitor {
                            sender: self.sender.clone(),
                        })?;
                    } else {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(())
            }
        }

        struct SnapshotSeqVisitor {
            sender: Sender<Result<Snapshot>>,
        }

        impl<'de> serde::de::DeserializeSeed<'de> for SnapshotSeqVisitor {
            type Value = ();
            fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_seq(self)
            }
        }

        impl<'de> serde::de::Visitor<'de> for SnapshotSeqVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("snapshots array")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut sender_closed = false;
                loop {
                    if sender_closed {
                        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                            continue;
                        }
                        break;
                    }

                    let Some(snapshot_value) = seq.next_element::<Value>()? else {
                        break;
                    };

                    // Parse each entry independently so one malformed snapshot can surface as
                    // an item-level error without aborting the entire stream.
                    match serde_json::from_value::<Snapshot>(snapshot_value) {
                        Ok(snapshot) => {
                            if self.sender.blocking_send(Ok(snapshot)).is_err() {
                                sender_closed = true;
                            }
                        }
                        Err(err) => {
                            if self.sender.blocking_send(Err(eyre!(err))).is_err() {
                                sender_closed = true;
                            }
                        }
                    }
                }
                Ok(())
            }
        }

        deserializer.deserialize_map(SnapshotRootVisitor { sender })
    }
}

impl DataSource for Repositories {
    fn name() -> String {
        "repositories".to_string()
    }

    fn aliases() -> Vec<&'static str> {
        vec!["repository"]
    }
}

impl DataSource for Snapshots {
    fn name() -> String {
        "snapshot".to_string()
    }
}
