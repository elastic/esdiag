/// For the `_alias` API
mod aliases;
/// For the `_data_streams` API
mod data_streams;
/// For the `_ilm/explain` API
mod ilm_explain;
/// For the `_settings` API
mod indices_settings;
/// For the `_nodes` API
mod nodes;
/// For the `_searchable_snapshots/cache/stats` API
mod searchable_snapshots_cache_stats;
/// For the root `/` API
mod version;

pub use aliases::*;
pub use data_streams::*;
pub use ilm_explain::*;
pub use indices_settings::*;
pub use nodes::*;
pub use searchable_snapshots_cache_stats::*;
pub use version::*;
