// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Role-typed exporter for the `Save` stage.
//!
//! A bundle exporter only accepts raw collection output. It cannot index
//! processed documents or send a bundle to a remote cluster.

use super::{ArchiveExporter, DirectoryExporter};
use crate::data::Uri;
use eyre::{Result, eyre};
use std::path::PathBuf;

#[derive(Clone)]
pub struct BundleExporter {
    inner: ArchiveExporter,
}

impl BundleExporter {
    pub fn for_collect(uri: Uri) -> Result<Self> {
        let inner = match uri {
            Uri::Directory(path) | Uri::File(path) => ArchiveExporter::Directory(DirectoryExporter::try_from(path)?),
            _ => return Err(eyre!("Collect requires a local directory output when --zip is not set")),
        };
        Ok(Self { inner })
    }

    pub fn archive(output_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            inner: ArchiveExporter::zip(output_dir)?,
        })
    }

    pub(crate) fn into_archive(self) -> ArchiveExporter {
        self.inner
    }
}

impl std::fmt::Display for BundleExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}
