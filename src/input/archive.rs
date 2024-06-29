use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub fn read_string(
    archive_path: &PathBuf,
    filename: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let zipfile = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(zipfile)?;
    let file_path = {
        let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
        while path.extension() != None {
            path.pop();
        }
        path.push(filename);
        path.to_str()
            .expect("Archive PathBuf to string failed")
            .to_string()
    };
    log::debug!("Reading {} from archive {:?}", file_path, archive_path);
    let file = archive.by_name(&file_path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut string = String::new();
    while let Some(line) = lines.next() {
        string.push_str(&line?);
    }
    Ok(string)
}
