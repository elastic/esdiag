use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Reads the contents of a specified file within a ZIP archive and returns it as a string.
///
/// # Arguments
///
/// * `archive_path` - A reference to the path of the ZIP archive.
/// * `filename` - The name of the file within the archive to read.
///
/// # Returns
///
/// A `Result` containing the file contents as a `String` if successful, or a boxed `Error` if an error occurs.
///
/// # Errors
///
/// This function will return an error if:
/// - The ZIP archive cannot be opened.
/// - The specified file does not exist within the archive.
/// - There is an issue reading from the archive.
///
/// # Example
///
/// ```rust
/// use std::path::PathBuf;
///
/// let archive_path = PathBuf::from("path/to/archive.zip");
/// let filename = "file_to_read.txt";
/// match read_string(&archive_path, filename) {
///     Ok(contents) => println!("File contents: {}", contents),
///     Err(e) => eprintln!("Error reading file: {}", e),
/// }
/// ```
///
/// # Panics
///
/// This function will panic if converting the archive path to a string fails.

pub fn read_string(
    archive_path: &PathBuf,
    filename: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;

    // Use the first file in the archive to determine the path
    let file_path = {
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        if path.extension() != None {
            path.pop();
        }
        path.push(filename);
        path.to_str()
            .expect("Archive PathBuf to string failed")
            .to_string()
    };

    // Read lines directly from the compressed file
    log::debug!("From archive {:?}, file \"{}\"", archive_path, file_path);
    let file = archive.by_name(&file_path)?;
    let read_lines = BufReader::new(file).lines();
    let string = read_lines.filter_map(Result::ok).collect::<String>();
    Ok(string)
}
