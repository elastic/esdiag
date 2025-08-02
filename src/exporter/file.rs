use crate::processor::{BatchResponse, DiagnosticReport, Identifiers, ProcessorSummary};

use super::Export;
use eyre::Result;
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
};

pub struct FileExporter {
    file: File,
    path: PathBuf,
    pub identifiers: Identifiers,
}

impl Clone for FileExporter {
    fn clone(&self) -> Self {
        Self {
            file: self.file.try_clone().expect("Failed to clone file"),
            path: self.path.clone(),
            identifiers: self.identifiers.clone(),
        }
    }
}

impl TryFrom<PathBuf> for FileExporter {
    type Error = eyre::Report;

    fn try_from(path: PathBuf) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        Ok(Self {
            file,
            path,
            identifiers: Identifiers::default(),
        })
    }
}

impl Export for FileExporter {
    fn with_identifiers(self, identifiers: Identifiers) -> Self {
        Self {
            identifiers,
            ..self
        }
    }

    async fn is_connected(&self) -> bool {
        let is_file = self.path.is_file();
        let filename = self.path.to_str().unwrap_or("");
        log::debug!("File {filename} is valid: {is_file}");
        is_file
    }

    async fn write(&self, index: String, docs: Vec<Value>) -> Result<ProcessorSummary> {
        let start_time = std::time::Instant::now();
        let mut summary = ProcessorSummary::new(index.clone());
        let mut batch = BatchResponse::new(docs.len() as u32);
        match &self.path.is_file() {
            false => {
                log::info!("Creating file {}", &self.path.display());
                std::fs::File::create(&self.path)?;
            }
            true => {
                log::debug!("File {} exists", &self.path.display());
            }
        }
        let mut writer = BufWriter::new(&self.file);
        let mut doc_count = 0;
        for doc in docs {
            serde_json::to_writer(&mut writer, &doc)?;
            writeln!(&mut writer)?;
            doc_count += 1;
        }
        writer.flush()?;
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::MetadataExt;
            batch.size = self.file.metadata()?.size() as u32;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            batch.size = self.file.metadata()?.size() as u32;
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::MetadataExt;
            batch.size = self.file.metadata()?.file_size() as u32;
        }
        batch.time = start_time.elapsed().as_millis() as u32;

        summary.add_batch(batch);
        log::info!("{}, created {} docs", index, doc_count);
        Ok(summary)
    }

    async fn save_report(&self, report: &DiagnosticReport) -> Result<()> {
        crate::data::save_file("report.json", report)
    }
}

impl std::fmt::Display for FileExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}
