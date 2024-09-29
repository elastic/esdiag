/// Data structures and types for serializing and deserializing
pub mod data;
/// Environment variables
pub mod env;
/// Manage saving and loading hosts from a YAML file
pub mod host;
/// Receive data from various sources
pub mod input;
/// Exports data to various destinations
pub mod output;
/// Data transformation and processing logic
pub mod processor;
/// Send pre-built assets (index templates, etc) to Elasticsearch
pub mod setup;
/// Classify an input string as a type of univeral resource identifier (URI)
pub mod uri;
