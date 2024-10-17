use super::Lookup;
use crate::data::elasticsearch::{Alias, AliasList};

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
