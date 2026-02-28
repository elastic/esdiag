use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=about.hbs");

    let notice_path = Path::new("NOTICE.txt");
    let sbom_path = Path::new("esdiag.spdx.json");
    let cargo_toml_path = Path::new("Cargo.toml");
    let about_hbs_path = Path::new("about.hbs");

    let should_generate = if !notice_path.exists() || !sbom_path.exists() {
        true
    } else {
        let notice_mtime = notice_path.metadata().and_then(|m| m.modified()).ok();
        let sbom_mtime = sbom_path.metadata().and_then(|m| m.modified()).ok();
        let cargo_mtime = cargo_toml_path.metadata().and_then(|m| m.modified()).ok();
        let about_mtime = about_hbs_path.metadata().and_then(|m| m.modified()).ok();

        match (notice_mtime, sbom_mtime, cargo_mtime, about_mtime) {
            (Some(nm), Some(sm), Some(cm), Some(am)) => cm > nm || am > nm || cm > sm,
            _ => true,
        }
    };

    if should_generate {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

        // Generate NOTICE.txt
        let output_about = Command::new(&cargo)
            .args(["about", "generate", "about.hbs"])
            .output()
            .expect("failed to execute cargo about. Is cargo-about installed?");

        if output_about.status.success() {
            std::fs::write(notice_path, output_about.stdout).expect("failed to write NOTICE.txt");
        } else {
            panic!(
                "cargo about failed: {}",
                String::from_utf8_lossy(&output_about.stderr)
            );
        }

        // Generate esdiag.spdx.json
        let output_sbom = Command::new(&cargo)
            .args(["sbom"])
            .output()
            .expect("failed to execute cargo sbom. Is cargo-sbom installed?");

        if output_sbom.status.success() {
            std::fs::write(sbom_path, output_sbom.stdout)
                .expect("failed to write esdiag.spdx.json");
        } else {
            panic!(
                "cargo sbom failed: {}",
                String::from_utf8_lossy(&output_sbom.stderr)
            );
        }
    }

    #[cfg(feature = "desktop")]
    {
        println!("cargo:rerun-if-changed=tauri.conf.json");
        println!("cargo:rerun-if-changed=icons");
        tauri_build::build();
    }
}
