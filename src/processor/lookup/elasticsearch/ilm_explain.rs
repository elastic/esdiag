use super::Lookup;
use crate::data::elasticsearch::{IlmExplain, IlmStats};

impl From<String> for Lookup<IlmStats> {
    fn from(string: String) -> Self {
        let ilm_explain: IlmExplain =
            serde_json::from_str(&string).expect("Failed to deserialize ilm_explain");
        Lookup::<IlmStats>::from(ilm_explain)
    }
}

impl From<IlmExplain> for Lookup<IlmStats> {
    fn from(mut ilm_explain: IlmExplain) -> Self {
        let mut lookup: Lookup<IlmStats> = Lookup::new();
        ilm_explain.indices.drain().for_each(|(index, ilm_stats)| {
            lookup.add(ilm_stats).with_name(&index);
        });

        log::debug!("lookup_ilm entries: {}", lookup.entries.len());
        lookup
    }
}
