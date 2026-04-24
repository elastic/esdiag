use std::process::{Command, Output};
use tempfile::TempDir;

fn setup_home() -> TempDir {
    let home = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(home.path().join(".esdiag")).expect("create config dir");
    home
}

fn run_esdiag(args: &[&str], home: &TempDir, extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", home.path().join(".esdiag").join("hosts.yml"))
        .env("ESDIAG_KEYSTORE", home.path().join(".esdiag").join("secrets.yml"))
        .env("LOG_LEVEL", "debug");

    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    cmd.output().expect("run esdiag")
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn agent_flag_emits_stderr_summary_without_info_logs() {
    let home = setup_home();
    let output = run_esdiag(&["--agent", "keystore", "status"], &home, &[]);
    assert_success(&output, "keystore status with --agent");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.is_empty(), "stdout should remain empty, got:\n{stdout}");
    assert!(
        stderr.contains("Keystore: locked"),
        "stderr should contain final summary, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "agent mode should suppress info logs, stderr was:\n{stderr}"
    );
}

#[test]
fn claudecode_auto_enables_agent_mode() {
    let home = setup_home();
    let output = run_esdiag(&["keystore", "status"], &home, &[("CLAUDECODE", "1")]);
    assert_success(&output, "keystore status with CLAUDECODE");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.is_empty(), "stdout should remain empty, got:\n{stdout}");
    assert!(
        stderr.contains("Keystore: locked"),
        "stderr should contain final summary, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "CLAUDECODE should enable warn-level suppression, stderr was:\n{stderr}"
    );
}

#[test]
fn debug_overrides_agent_warn_level() {
    let home = setup_home();
    let output = run_esdiag(&["--agent", "--debug", "keystore", "status"], &home, &[]);
    assert_success(&output, "keystore status with --agent --debug");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.is_empty(), "stdout should remain empty, got:\n{stdout}");
    assert!(
        stderr.contains("Keystore: locked"),
        "stderr should contain final summary, got:\n{stderr}"
    );
}

#[test]
fn final_summary_is_printed_without_agent_when_logs_are_warn() {
    let home = setup_home();
    let output = run_esdiag(&["keystore", "status"], &home, &[("LOG_LEVEL", "warn")]);
    assert_success(&output, "keystore status with warn log level");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.is_empty(), "stdout should remain empty, got:\n{stdout}");
    assert!(
        stderr.contains("Keystore: locked"),
        "stderr should contain final summary, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("CLI unlock: inactive"),
        "warn log level should suppress info logs, stderr was:\n{stderr}"
    );
}
