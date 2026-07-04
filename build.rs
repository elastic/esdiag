use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

#[cfg(feature = "desktop")]
use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=about.hbs");
    println!("cargo:rerun-if-env-changed=ESDIAG_GENERATE_NOTICE");
    println!("cargo:rerun-if-env-changed=ESDIAG_GENERATE_SBOM");

    build_kibana_assets_bundle();

    let notice_path = Path::new("NOTICE.txt");
    let sbom_path = Path::new("esdiag.spdx.json");
    let cargo_toml_path = Path::new("Cargo.toml");
    let about_hbs_path = Path::new("about.hbs");
    let generate_notice = env_flag("ESDIAG_GENERATE_NOTICE", true);
    let generate_sbom = env_flag("ESDIAG_GENERATE_SBOM", false);

    let should_generate_notice = if !generate_notice {
        false
    } else if !notice_path.exists() {
        true
    } else {
        let notice_mtime = notice_path.metadata().and_then(|m| m.modified()).ok();
        let cargo_mtime = cargo_toml_path.metadata().and_then(|m| m.modified()).ok();
        let about_mtime = about_hbs_path.metadata().and_then(|m| m.modified()).ok();

        match (notice_mtime, cargo_mtime, about_mtime) {
            (Some(nm), Some(cm), Some(am)) => cm > nm || am > nm,
            _ => true,
        }
    };

    let should_generate_sbom = if !generate_sbom {
        false
    } else if !sbom_path.exists() {
        true
    } else {
        let sbom_mtime = sbom_path.metadata().and_then(|m| m.modified()).ok();
        let cargo_mtime = cargo_toml_path.metadata().and_then(|m| m.modified()).ok();

        match (sbom_mtime, cargo_mtime) {
            (Some(sm), Some(cm)) => cm > sm,
            _ => true,
        }
    };

    if should_generate_notice || should_generate_sbom {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

        if should_generate_notice {
            let output_about = Command::new(&cargo)
                .args(["about", "generate", "about.hbs"])
                .output()
                .expect("failed to execute cargo about. Is cargo-about installed?");

            if output_about.status.success() {
                std::fs::write(notice_path, output_about.stdout).expect("failed to write NOTICE.txt");
            } else {
                panic!("cargo about failed: {}", String::from_utf8_lossy(&output_about.stderr));
            }
        }

        if should_generate_sbom {
            let output_sbom = Command::new(&cargo)
                .args(["sbom"])
                .output()
                .expect("failed to execute cargo sbom. Is cargo-sbom installed?");

            if output_sbom.status.success() {
                std::fs::write(sbom_path, output_sbom.stdout).expect("failed to write esdiag.spdx.json");
            } else {
                panic!("cargo sbom failed: {}", String::from_utf8_lossy(&output_sbom.stderr));
            }
        }
    }

    #[cfg(feature = "desktop")]
    {
        let manifest_dir =
            env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR for desktop build");
        let manifest_path = Path::new(&manifest_dir);
        let desktop_dir = manifest_path.join("desktop");

        emit_rerun_if_changed(manifest_path, &manifest_path.join("tauri.conf.json"));
        emit_rerun_if_changed(manifest_path, &desktop_dir.join("capabilities"));
        emit_rerun_if_changed(manifest_path, &desktop_dir.join("frontend-dist"));
        emit_rerun_if_changed(manifest_path, &desktop_dir.join("icons"));
        emit_rerun_if_changed(manifest_path, &desktop_dir.join("packaging"));

        tauri_build::try_build(
            tauri_build::Attributes::new()
                .capabilities_path_pattern("desktop/capabilities/**/*")
                .codegen(tauri_build::CodegenContext::new().config_path("tauri.conf.json")),
        )
        .expect("failed to build desktop Tauri context");
    }
}

fn build_kibana_assets_bundle() {
    let out_dir = env::var("OUT_DIR").expect("missing OUT_DIR");
    let output_path = Path::new(&out_dir).join("kibana-assets.zip");
    let mut output = File::create(&output_path).expect("failed to create Kibana assets bundle");
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let assets_root = Path::new("assets");
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
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).expect("failed to read Kibana asset");
        zip.write_all(&contents)
            .expect("failed to write Kibana asset to bundle");
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

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(default)
}

#[cfg(feature = "desktop")]
fn emit_rerun_if_changed(repo_root: &Path, path: &Path) {
    let display_path = path.strip_prefix(repo_root).unwrap_or(path);
    println!("cargo:rerun-if-changed={}", display_path.display());

    if !path.is_dir() {
        return;
    }

    for entry in fs::read_dir(path).expect("failed to read rerun-if-changed directory") {
        let entry = entry.expect("failed to read rerun-if-changed entry");
        emit_rerun_if_changed(repo_root, &entry.path());
    }
}
