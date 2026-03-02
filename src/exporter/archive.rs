use super::DirectoryExporter;
use eyre::{Result, eyre};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
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
            Self::Directory(exporter) => exporter.save(path, content).await,
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
            Self::Directory(exporter) => std::path::PathBuf::from(exporter.to_string()).is_dir(),
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
        if !output_dir.exists() {
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
        let entry = normalize_archive_path(path.as_path());
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|_| eyre!("Failed to acquire zip writer lock"))?;
        let writer = writer_guard
            .as_mut()
            .ok_or_else(|| eyre!("Zip output is not initialized"))?;

        writer.start_file(entry, SimpleFileOptions::default())?;
        writer.write_all(content.as_bytes())?;
        Ok(())
    }

    pub fn finalize(&self) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|_| eyre!("Failed to acquire zip writer lock"))?;

        if let Some(writer) = writer_guard.take() {
            writer.finish()?;
        }
        Ok(())
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

fn normalize_archive_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;
    use zip::ZipArchive;

    #[test]
    fn normalize_archive_path_uses_forward_slashes() {
        let normalized = normalize_archive_path(Path::new(r"\api\stats\nodes.json"));
        assert_eq!(normalized, "api/stats/nodes.json");
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
