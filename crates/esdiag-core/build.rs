// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use std::env;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"));
    let assets_dir = asset_root(&manifest_dir, "assets", "assets");
    let skill_dir = asset_root(&manifest_dir, ".agents/skills/esdiag", "skill");

    println!("cargo:rerun-if-changed={}", assets_dir.display());
    println!("cargo:rerun-if-changed={}", skill_dir.display());
    println!("cargo:rustc-env=ESDIAG_ASSETS_DIR={}", assets_dir.display());
    println!("cargo:rustc-env=ESDIAG_SKILL_DIR={}", skill_dir.display());

    if env::var_os("CARGO_FEATURE_SETUP").is_some() {
        build_kibana_assets_bundle(&manifest_dir.join(&assets_dir));
    }
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

fn build_kibana_assets_bundle(assets_root: &Path) {
    let out_dir = env::var("OUT_DIR").expect("missing OUT_DIR");
    let output_path = Path::new(&out_dir).join("kibana-assets.zip");
    let mut output = File::create(&output_path).expect("failed to create Kibana assets bundle");
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let kibana_root = assets_root.join("kibana");
    let mut files = Vec::new();
    let mut watched_paths = Vec::new();
    collect_kibana_asset_paths(&kibana_root, &mut files, &mut watched_paths);
    files.sort();
    watched_paths.sort();
    watched_paths.dedup();

    for path in watched_paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    for path in files {
        let relative_path = path
            .strip_prefix(assets_root)
            .expect("Kibana asset should be under assets directory");
        let archive_path = relative_path.to_string_lossy().replace('\\', "/");

        zip.start_file(archive_path, options)
            .expect("failed to start Kibana assets bundle entry");

        let mut file = File::open(&path).expect("failed to open Kibana asset");
        io::copy(&mut file, &mut zip).expect("failed to write Kibana asset to bundle");
    }

    zip.finish().expect("failed to finish Kibana assets bundle");
}

fn collect_kibana_asset_paths(path: &Path, files: &mut Vec<PathBuf>, watched_paths: &mut Vec<PathBuf>) {
    watched_paths.push(path.to_path_buf());

    for entry in std::fs::read_dir(path).expect("failed to read Kibana asset directory") {
        let entry = entry.expect("failed to read Kibana asset directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_kibana_asset_paths(&path, files, watched_paths);
        } else {
            watched_paths.push(path.clone());
            files.push(path);
        }
    }
}
