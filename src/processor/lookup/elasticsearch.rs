/// Lookup for `_alias`
pub mod alias;
/// Lookup for `_data_streams`
pub mod data_stream;
/// Lookup for `_ilm/explian`
pub mod ilm_explain;
/// Lookup for `_settings`
pub mod index;
/// Lookup for `_nodes`
pub mod node;
/// Lookup for `_searchable_snapshots/cache/stats`
pub mod shared_cache;

pub use super::Lookup;
