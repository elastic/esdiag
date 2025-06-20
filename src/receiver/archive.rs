use eyre::Result;
use std::{
    io::{Read, Seek},
    path::PathBuf,
};
use zip::ZipArchive;

mod bytes;
mod file;

pub use bytes::*;
pub use file::*;

pub fn trim_to_working_directory(path: &mut PathBuf) {
    // Drop any filename
    if path.extension() != None {
        path.pop();
    }
    // Drop any known subdirectories
    if path.ends_with("cat")
        || path.ends_with("commercial")
        || path.ends_with("docker")
        || path.ends_with("syscalls")
        || path.ends_with("java")
        || path.ends_with("logs")
    {
        path.pop();
    }
}

pub fn resolve_archive_path<A: Read + Seek>(
    subdir: Option<&PathBuf>,
    archive: &mut ZipArchive<A>,
    filename: &str,
) -> Result<String> {
    let full_path = match subdir {
        // Ugly hack to make ECK bundles with double-slashed paths work
        // This will break if the sub-paths are fixed in the ECK bundles
        Some(subdir) => format!("{}//{}", subdir.display(), filename),
        None => {
            let mut path = PathBuf::from(archive.by_index(0)?.name().to_string());
            trim_to_working_directory(&mut path);
            let path = path.join(filename);
            format!("{}", path.display())
        }
    };
    Ok(full_path)
}
