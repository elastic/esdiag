use flate2::write::GzEncoder;
use flate2::Compression;
use std::env;
use std::fs::File;
use std::path::Path;
use tar::Builder;

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR environment variable not set");
    let dest_path = Path::new(&out_dir).join("assets.tar.gz");

    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=build.rs");

    let assets_dir = Path::new("assets");
    if !assets_dir.exists() || !assets_dir.is_dir() {
        panic!(
            "Build error: expected an 'assets' directory at {:?}, but it was not found or is not a directory",
            assets_dir
        );
    }

    let tar_gz = File::create(&dest_path).expect("failed to create assets.tar.gz at dest_path");
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    tar.append_dir_all("", assets_dir)
        .expect("failed to append assets_dir to tar archive");

    let enc = tar
        .into_inner()
        .expect("failed to finalize tar archive writer for assets.tar.gz");
    enc.finish()
        .expect("failed to finish gzip compression for assets.tar.gz");
}
