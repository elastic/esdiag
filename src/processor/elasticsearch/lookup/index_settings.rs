use crate::data::{
    diagnostic::Lookup,
    elasticsearch::{IndexSettings, IndicesSettings},
};
use color_eyre::eyre::Result;

impl From<IndicesSettings> for Lookup<IndexSettings> {
    fn from(mut indices_settings: IndicesSettings) -> Self {
        let mut lookup = Lookup::<IndexSettings>::new();
        indices_settings.drain().for_each(|(name, settings)| {
            let mut index = settings.index();
            index.set_store_config();
            let id = index.uuid.clone();
            lookup.add(index).with_name(&name).with_id(&id);
        });
        lookup
    }
}

impl From<Result<IndicesSettings>> for Lookup<IndexSettings> {
    fn from(indices_settings: Result<IndicesSettings>) -> Self {
        match indices_settings {
            Ok(indices_settings) => Lookup::<IndexSettings>::from(indices_settings),
            Err(e) => {
                log::warn!("Failed to parse IndicesSettings: {}", e);
                Lookup::new()
            }
        }
    }
}
