use super::{Product, Source};
use crate::output::Target;
use crate::setup::Asset;
use include_dir::{include_dir, Dir};
use serde_yaml;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// Subdirectory for templates and configs files
pub static ASSETS_DIR: Dir = include_dir!("assets");
pub static ELASTICSEARCH_ASSETS: &str = "elasticsearch/assets.yml";
pub static ELASTICSEARCH_SOURCES: &str = "elasticsearch/sources.yml";

/// Reads the contents of a specified file and returns it as a string.
///
/// # Arguments
///
/// * `file_path` - A reference to the path of the file to read.
///
/// # Returns
///
/// A `Result` containing the file contents as a `String` if successful, or a boxed `Error` if an error occurs.
///
/// # Errors
///
/// This function will return an error if:
/// - The file cannot be opened.
/// - There is an issue reading from the file.
///
/// # Example
///
/// ```rust
/// use std::path::PathBuf;
///
/// let file_path = PathBuf::from("path/to/file.txt");
/// match read_string(&file_path) {
///     Ok(contents) => println!("File contents: {}", contents),
///     Err(e) => eprintln!("Error reading file: {}", e),
/// }
/// ```

pub fn read_string(file_path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    log::debug!("Reading file: {:?}", file_path);
    let file = File::open(file_path)?;
    let read_lines = BufReader::new(file).lines();
    let string = read_lines.filter_map(Result::ok).collect::<String>();
    Ok(string)
}

/// Parses the `sources.yml` file for a given product and returns its contents as a `HashMap`.
///
/// # Arguments
///
/// * `product` - A reference to the `Product` for which the `sources.yml` file should be parsed.
///
/// # Returns
///
/// A `Result` containing a `HashMap` with `String` keys and `Source` values if successful,
/// or a boxed `Error` if an error occurs.
///
/// # Errors
///
/// This function will return an error if:
/// - The `sources.yml` file is not found for the specified product.
/// - The `sources.yml` file cannot be parsed.
/// - The specified product is not implemented.
///
/// # Example
///
/// ```rust
/// match parse_sources_yml(&Product::Elasticsearch) {
///     Ok(sources) => println!("Parsed sources: {:?}", sources),
///     Err(e) => eprintln!("Error parsing sources.yml: {}", e),
/// }
/// ```

pub fn parse_sources_yml(
    product: &Product,
) -> Result<HashMap<String, Source>, Box<dyn std::error::Error>> {
    log::debug!("Parsing sources.yml");
    let file = match product {
        Product::Elasticsearch => ASSETS_DIR
            .get_file(ELASTICSEARCH_SOURCES)
            .ok_or(format!("Error reading {ELASTICSEARCH_SOURCES}"))?,
        _ => return Err(format!("{} not yet implemented", product).into()),
    };
    let sources = serde_yaml::from_slice(file.contents())?;
    Ok(sources)
}

/// Parses the `assets.yml` file for a given target and returns its contents as a `Vec` of `Asset`.
///
/// # Arguments
///
/// * `target` - A reference to the `Target` for which the `assets.yml` file should be parsed.
///
/// # Returns
///
/// A `Result` containing a `Vec` of `Asset` if successful,
/// or a boxed `Error` if an error occurs.
///
/// # Errors
///
/// This function will return an error if:
/// - The `assets.yml` file is not found for the specified target.
/// - The `assets.yml` file cannot be parsed.
/// - The specified target is not implemented.
///
/// # Example
///
/// ```rust
/// match parse_assets_yml(&Target::Elasticsearch(SomeVersion)) {
///     Ok(assets) => println!("Parsed assets: {:?}", assets),
///     Err(e) => eprintln!("Error parsing assets.yml: {}", e),
/// }
/// ```

pub fn parse_assets_yml(target: &Target) -> Result<Vec<Asset>, Box<dyn std::error::Error>> {
    let file = match target {
        Target::Elasticsearch(_) => ASSETS_DIR
            .get_file(ELASTICSEARCH_ASSETS)
            .ok_or(format!("Error reading {ELASTICSEARCH_ASSETS}"))?,
        _ => return Err("Application not implemented".into()),
    };
    let assets = serde_yaml::from_slice(file.contents())?;
    Ok(assets)
}
