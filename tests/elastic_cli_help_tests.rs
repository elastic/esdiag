use std::process::Command;

#[test]
fn elastic_cli_invocation_without_subcommand_prints_extension_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .env("ESDIAG_ELASTIC_CLI", "1")
        .output()
        .expect("run esdiag");

    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Elastic CLI extension examples:"));
    assert!(stdout.contains("elastic diag collect .es ./out"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("No subcommand provided"));
}
