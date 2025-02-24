/// Authentication methods
mod auth;
/// Wrapper for building Elasticsearch connections
mod elasticsearch;
/// Manage saving and loading hosts from a YAML file
mod known_host;

pub use auth::{Auth, AuthType};
pub use elasticsearch::ElasticsearchBuilder;
pub use known_host::{KnownHost, KnownHostBuilder};
