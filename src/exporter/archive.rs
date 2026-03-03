use super::DirectoryExporter;
use eyre::{Result, eyre};
use std::fs::File;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[derive(Clone)]
pub enum ArchiveExporter {
    Directory(DirectoryExporter),
    Zip(ZipArchiveExporter),
}

impl ArchiveExporter {
    pub fn zip(output_dir: PathBuf) -> Result<Self> {
        Ok(Self::Zip(ZipArchiveExporter::new(output_dir)?))
    }

    pub fn with_archive_name(self, archive_name: &str) -> Result<Self> {
        match self {
            Self::Directory(exporter) => Ok(Self::Directory(
                exporter.collection_directory(archive_name.to_string())?,
            )),
            Self::Zip(exporter) => Ok(Self::Zip(
                exporter.with_filename(format!("{archive_name}.zip"))?,
            )),
        }
    }

    pub async fn save(&self, path: PathBuf, content: String) -> Result<()> {
        match self {
            Self::Directory(exporter) => {
                validate_relative_output_path(path.as_path())?;
                exporter.save(path, content).await
            }
            Self::Zip(exporter) => exporter.save(path, content).await,
        }
    }

    pub fn finalize(&self) -> Result<()> {
        match self {
            Self::Directory(_) => Ok(()),
            Self::Zip(exporter) => exporter.finalize(),
        }
    }

    pub fn is_connected(&self) -> bool {
        match self {
            Self::Directory(exporter) => exporter.is_dir(),
            Self::Zip(exporter) => exporter.is_connected(),
        }
    }
}

impl std::fmt::Display for ArchiveExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Directory(exporter) => write!(f, "{}", exporter),
            Self::Zip(exporter) => write!(f, "{}", exporter),
        }
    }
}

#[derive(Clone)]
pub struct ZipArchiveExporter {
    output_dir: PathBuf,
    output_file: Arc<Mutex<Option<PathBuf>>>,
    writer: Arc<Mutex<Option<ZipWriter<File>>>>,
}

impl ZipArchiveExporter {
    pub fn new(output_dir: PathBuf) -> Result<Self> {
        if output_dir.exists() {
            if !output_dir.is_dir() {
                return Err(eyre!(
                    "Zip output destination must be a directory: {}",
                    output_dir.display()
                ));
            }
        } else {
            std::fs::create_dir_all(&output_dir)?;
        }
        Ok(Self {
            output_dir,
            output_file: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_filename(self, filename: String) -> Result<Self> {
        let output_file = self.output_dir.join(filename);
        if let Some(parent) = output_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&output_file)?;
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|_| eyre!("Failed to acquire zip writer lock"))?;
        *writer_guard = Some(ZipWriter::new(file));

        let mut output_file_guard = self
            .output_file
            .lock()
            .map_err(|_| eyre!("Failed to acquire output file lock"))?;
        *output_file_guard = Some(output_file);
        drop(output_file_guard);
        drop(writer_guard);

        Ok(self)
    }

    pub async fn save(&self, path: PathBuf, content: String) -> Result<()> {
        let entry = normalize_archive_path(path.as_path())?;
        let writer = Arc::clone(&self.writer);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut writer_guard = writer
                .lock()
                .map_err(|_| eyre!("Failed to acquire zip writer lock"))?;
            let writer = writer_guard
                .as_mut()
                .ok_or_else(|| eyre!("Zip output is not initialized"))?;
            writer.start_file(entry, SimpleFileOptions::default())?;
            writer.write_all(content.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| eyre!("Failed to join zip write task: {}", e))??;
        Ok(())
    }

    pub fn finalize(&self) -> Result<()> {
        let finalize_inner = || -> Result<()> {
            let mut writer_guard = self
                .writer
                .lock()
                .map_err(|_| eyre!("Failed to acquire zip writer lock"))?;

            if let Some(writer) = writer_guard.take() {
                writer.finish()?;
            }
            Ok(())
        };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(finalize_inner)
            }
            _ => finalize_inner(),
        }
    }

    fn is_connected(&self) -> bool {
        self.output_file
            .lock()
            .ok()
            .and_then(|path| path.clone())
            .map(|path| path.is_file())
            .unwrap_or(false)
    }
}

impl std::fmt::Display for ZipArchiveExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self
            .output_file
            .lock()
            .ok()
            .and_then(|path| path.clone())
            .unwrap_or_else(|| self.output_dir.clone());
        write!(f, "{}", output.display())
    }
}

fn normalize_archive_path(path: &Path) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_string_lossy().replace('\\', "/");
                for part in segment.split('/') {
                    if part.is_empty() || part == "." {
                        continue;
                    }
                    if part == ".." {
                        return Err(eyre!(
                            "Archive path cannot contain parent directory components: {}",
                            path.display()
                        ));
                    }
                    if part.ends_with(':') {
                        return Err(eyre!(
                            "Archive path cannot contain drive prefixes: {}",
                            path.display()
                        ));
                    }
                    parts.push(part.to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(eyre!(
                    "Archive path cannot contain parent directory components: {}",
                    path.display()
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(eyre!(
                    "Archive path must be relative and without root/prefix: {}",
                    path.display()
                ));
            }
        }
    }

    if parts.is_empty() {
        return Err(eyre!("Archive path is empty: {}", path.display()));
    }

    Ok(parts.join("/"))
}

fn validate_relative_output_path(path: &Path) -> Result<()> {
    if path.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return Err(eyre!(
            "Output path must be relative and remain within the destination directory: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;
    use zip::ZipArchive;

    #[test]
    fn normalize_archive_path_uses_forward_slashes() {
        let normalized =
            normalize_archive_path(Path::new(r"api\stats\nodes.json")).expect("normalize path");
        assert_eq!(normalized, "api/stats/nodes.json");
    }

    #[test]
    fn normalize_archive_path_rejects_parent_components() {
        let err = normalize_archive_path(Path::new("../api/stats.json")).expect_err("reject path");
        assert!(err.to_string().contains("parent directory"));
    }

    #[test]
    fn validate_relative_output_path_rejects_parent_components() {
        let err =
            validate_relative_output_path(Path::new("../api/stats.json")).expect_err("reject path");
        assert!(err.to_string().contains("relative"));
    }

    #[test]
    fn zip_archive_exporter_new_rejects_file_output_path() {
        let dir = tempdir().expect("temp dir");
        let file_path = dir.path().join("not-a-directory");
        File::create(&file_path).expect("create file");

        let err = ZipArchiveExporter::new(file_path)
            .err()
            .expect("reject non-directory path");
        assert!(err.to_string().contains("must be a directory"));
    }

    #[tokio::test]
    async fn zip_archive_exporter_writes_entries() {
        let dir = tempdir().expect("temp dir");
        let exporter =
            ZipArchiveExporter::new(dir.path().to_path_buf()).expect("create zip exporter");
        let exporter = exporter
            .with_filename("diagnostic.zip".to_string())
            .expect("initialize filename");

        exporter
            .save(
                PathBuf::from("cluster/health.json"),
                "{\"ok\":true}".to_string(),
            )
            .await
            .expect("save entry");
        exporter.finalize().expect("finalize archive");

        let file = File::open(dir.path().join("diagnostic.zip")).expect("open archive");
        let mut archive = ZipArchive::new(file).expect("read archive");
        let mut entry = archive
            .by_name("cluster/health.json")
            .expect("entry exists");
        let mut body = String::new();
        entry.read_to_string(&mut body).expect("read entry");
        assert_eq!(body, "{\"ok\":true}");
    }
}
