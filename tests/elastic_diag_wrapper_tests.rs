use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn node_path() -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|dir| dir.join("node"))
            .find(|candidate| candidate.is_file())
    })
}

fn wrapper_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("bin/elastic-diag")
}

#[cfg(unix)]
#[test]
fn elastic_diag_process_help_reaches_esdiag_command_surface() {
    let Some(node) = node_path() else {
        eprintln!("skipping wrapper test because node was not found on PATH");
        return;
    };
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let shim = tmp.path().join("esdiag");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_esdiag"), &shim).expect("symlink esdiag");

    let output = Command::new(node)
        .arg(wrapper_path())
        .arg("process")
        .arg("--help")
        .env("PATH", tmp.path())
        .output()
        .expect("run wrapper");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("Usage: esdiag process"));
    assert!(stdout.contains("Source to read diagnostic data from"));
}

#[cfg(unix)]
#[test]
fn elastic_diag_wrapper_sets_marker_forwards_args_and_preserves_status() {
    let Some(node) = node_path() else {
        eprintln!("skipping wrapper test because node was not found on PATH");
        return;
    };
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let fake_esdiag = tmp.path().join("esdiag");
    let capture = tmp.path().join("capture.txt");
    fs::write(
        &fake_esdiag,
        r#"#!/bin/sh
printf '%s\n' "$ESDIAG_ELASTIC_CLI" > "$ESDIAG_WRAPPER_CAPTURE"
printf '%s\n' "$@" >> "$ESDIAG_WRAPPER_CAPTURE"
exit 7
"#,
    )
    .expect("write fake esdiag");
    fs::set_permissions(&fake_esdiag, fs::Permissions::from_mode(0o755)).expect("chmod fake esdiag");

    let output = Command::new(node)
        .arg(wrapper_path())
        .arg("process")
        .arg("--help")
        .env("PATH", tmp.path())
        .env("ESDIAG_WRAPPER_CAPTURE", &capture)
        .output()
        .expect("run wrapper");

    assert_eq!(output.status.code(), Some(7));
    let captured = fs::read_to_string(capture).expect("read capture");
    assert_eq!(captured, "1\nprocess\n--help\n");
}

#[test]
fn elastic_diag_wrapper_reports_missing_esdiag() {
    let Some(node) = node_path() else {
        eprintln!("skipping wrapper test because node was not found on PATH");
        return;
    };
    let tmp = tempfile::TempDir::new().expect("temp dir");

    let output = Command::new(node)
        .arg(wrapper_path())
        .arg("--help")
        .env("PATH", tmp.path())
        .output()
        .expect("run wrapper");

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8(output.stderr).expect("stderr");
    assert!(stderr.contains("Unable to find `esdiag` on PATH."));
    assert!(stderr.contains("cargo install --git https://github.com/elastic/esdiag.git"));
}
