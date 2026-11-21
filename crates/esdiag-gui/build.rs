// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let docs_dir = asset_root(&manifest_dir, "docs", "docs");

    println!("cargo:rerun-if-changed={}", docs_dir.display());
    println!("cargo:rustc-env=ESDIAG_DOCS_DIR={}", docs_dir.display());
}

/// Selects the canonical repository directory in a checkout and the copy under
/// `bundled/` in a packaged crate, relative to the manifest directory because
/// rust-embed's `compression` feature rejects absolute folders. Cargo writes
/// `.cargo_vcs_info.json` only into packaged crates.
fn asset_root(manifest_dir: &Path, canonical: &str, bundled: &str) -> PathBuf {
    let canonical = Path::new("../..").join(canonical);
    if !manifest_dir.join(".cargo_vcs_info.json").exists() && manifest_dir.join(&canonical).is_dir() {
        return canonical;
    }
    let bundled = Path::new("bundled").join(bundled);
    assert!(
        manifest_dir.join(&bundled).is_dir(),
        "missing {}; run bin/sync-crate-assets.sh before packaging",
        manifest_dir.join(&bundled).display()
    );
    bundled
}
