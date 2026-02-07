// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{super::Lookup, Alias, AliasList};
use eyre::Result;

impl From<String> for Lookup<Alias> {
    fn from(string: String) -> Self {
        let alias_list: AliasList =
            serde_json::from_str(&string).expect("Failed to parse AliasData");
        Lookup::<Alias>::from(alias_list)
    }
}

impl From<AliasList> for Lookup<Alias> {
    fn from(mut alias_list: AliasList) -> Self {
        let mut lookup: Lookup<Alias> = Lookup::new();
        alias_list.drain().for_each(|(index_name, mut aliases)| {
            aliases
                .aliases
                .drain()
                .for_each(|(alias_name, alias_settings)| {
                    let alias = Alias::from(alias_settings).with_name(alias_name);
                    lookup.add(alias).with_name(&index_name);
                });
        });
        log::debug!("lookup alias entries: {}", lookup.len());
        lookup
    }
}

impl From<Result<AliasList>> for Lookup<Alias> {
    fn from(alias_list: Result<AliasList>) -> Self {
        match alias_list {
            Ok(alias_list) => Lookup::<Alias>::from_parsed(alias_list),
            Err(e) => {
                log::warn!("Failed to parse AliasList: {}", e);
                Lookup::new()
            }
        }
    }
}
