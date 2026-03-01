// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Authentication methods
mod auth;
/// Manage saving and loading hosts from a YAML file
mod known_host;
/// Elastic stack products
mod product;
/// Application Settings
pub mod settings;
/// Universal resource identifiers
mod uri;

pub use auth::{Auth, AuthType};
pub use known_host::{ElasticCloud, KnownHost, KnownHostBuilder};
pub use product::Product;
pub use settings::Settings;
pub use uri::Uri;

use crate::env;
use eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

/// Save an arbitrary serializable object to a file
pub fn save_file<T: Serialize>(filename: &str, content: &T) -> Result<()> {
    let home_file = PathBuf::from(env::get_string("HOME")?)
        .join(env::get_string("ESDIAG_HOME")?)
        .join("last_run")
        .join(filename);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(home_file)?;
    let body = serde_json::to_string(&content)?;
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

/// The standard deserializer from serde_json does not deserializing u64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.
pub fn u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_u64()),
        Value::String(s) => Ok(s.parse::<u64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

/// The standard deserializer from serde_json does not deserializing i64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.
pub fn i64_from_string<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_i64()),
        Value::String(s) => Ok(s.parse::<i64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

pub fn map_as_vec_entries<'de, D, T>(deserializer: D) -> Result<Vec<(String, T)>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for MapVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<(String, T)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut values = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some(key) = map.next_key()? {
                let value = map.next_value()?;
                values.push((key, value));
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

pub fn option_map_as_vec_entries<'de, D, T>(
    deserializer: D,
) -> Result<Option<Vec<(String, T)>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct OptionMapVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for OptionMapVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Option<Vec<(String, T)>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an optional map")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            map_as_vec_entries(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionMapVisitor(std::marker::PhantomData))
}
