// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Role-typed exporter for processed diagnostic documents.
//!
//! Construction rejects the legacy archive variant, so a processing pipeline
//! cannot accidentally write processed documents into a raw collection bundle.

use super::Exporter;
use crate::data::{KnownHost, Uri};
use eyre::{Result, eyre};

#[derive(Clone, Default)]
pub struct DocumentExporter {
    inner: Exporter,
}

impl DocumentExporter {
    pub(crate) fn into_inner(self) -> Exporter {
        self.inner
    }
}

impl TryFrom<Exporter> for DocumentExporter {
    type Error = eyre::Report;

    fn try_from(exporter: Exporter) -> Result<Self> {
        match exporter {
            Exporter::Archive(_) => Err(eyre!("Raw bundle exporters cannot export processed documents")),
            inner => Ok(Self { inner }),
        }
    }
}

impl TryFrom<Uri> for DocumentExporter {
    type Error = eyre::Report;

    fn try_from(uri: Uri) -> Result<Self> {
        Self::try_from(Exporter::try_from(uri)?)
    }
}

impl TryFrom<KnownHost> for DocumentExporter {
    type Error = eyre::Report;

    fn try_from(host: KnownHost) -> Result<Self> {
        Self::try_from(Exporter::try_from(host)?)
    }
}

impl std::fmt::Display for DocumentExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exporter::ArchiveExporter;

    #[test]
    fn raw_bundle_exporter_cannot_become_document_exporter() {
        let output = tempfile::tempdir().expect("output");
        let legacy = Exporter::Archive(ArchiveExporter::zip(output.path().to_path_buf()).expect("archive exporter"));

        let error = DocumentExporter::try_from(legacy)
            .err()
            .expect("archive role must be rejected");

        assert!(error.to_string().contains("cannot export processed documents"));
    }
}
