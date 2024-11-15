/// Diagnostic bundle types and data structures
pub mod diagnostic;
/// Elasticsearch data types and structures
pub mod elasticsearch;
/// Logstash data types and structures
pub mod logstash;
/// Classify an input string as a type of univeral resource identifier (URI)
pub mod uri;

pub use uri::Uri;

// ------ Utility Function -------
use crate::env;
use color_eyre::eyre::Result;
use serde::Serialize;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

/// Save an arbitrary serializable object to a file
pub fn save_file<T: Serialize>(filename: &str, content: &T) -> Result<()> {
    let home_file = PathBuf::from(env::get_string("HOME")?)
        .join(env::get_string("ESDIAG_HOME")?)
        .join("last_run")
        .join(filename);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(home_file)?;
    let body = serde_json::to_string(&content)?;
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}
