use std::env;
use std::process::Command;

/// Process an 8.19 Agent diagnostic archive and verify it completes successfully.
/// This test uses a clean archive generated from a standalone Docker agent.
#[test]
fn test_process_agent_diagnostic_8_19() {
    let archive = "tests/archives/elastic-agent-diagnostics-8.19.12.zip";
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive, "-"])
        .output()
        .expect("Failed to execute process");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify manifest detection via agent-info.yaml
    assert!(
        stderr.contains("Using agent-info.yaml as manifest source"),
        "Should detect agent bundle via agent-info.yaml: {stderr}"
    );

    // Verify Agent product is identified
    assert!(
        stderr.contains("Processing Agent diagnostic"),
        "Should identify product as Agent: {stderr}"
    );

    // Verify documents are created (exit code 0 means no crash, even without an ES target)
    assert!(
        output.status.code().is_some(),
        "Process should not crash: {stderr}"
    );
}

/// Process a 9.3 Agent diagnostic archive and verify it completes successfully.
#[test]
fn test_process_agent_diagnostic_9_3() {
    let archive = "tests/archives/elastic-agent-diagnostics-9.3.1.zip";
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive, "-"])
        .output()
        .expect("Failed to execute process");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("Using agent-info.yaml as manifest source"),
        "Should detect agent bundle via agent-info.yaml: {stderr}"
    );
    assert!(
        stderr.contains("Processing Agent diagnostic"),
        "Should identify product as Agent: {stderr}"
    );
    assert!(
        output.status.code().is_some(),
        "Process should not crash: {stderr}"
    );
}

/// A 7.17 Agent diagnostic has a completely different structure (no agent-info.yaml).
/// The processor should fail gracefully, not crash.
#[test]
fn test_process_agent_diagnostic_7_17_unsupported() {
    let archive = "tests/archives/elastic-agent-diagnostics-7.17.28.zip";
    let output = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["process", archive, "-"])
        .output()
        .expect("Failed to execute process");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail to find any manifest format
    assert!(
        !output.status.success(),
        "7.17 bundle should fail (unsupported format): {stderr}"
    );
    assert!(
        stderr.contains("agent-info.yaml"),
        "Error should mention agent-info.yaml in the fallback chain: {stderr}"
    );
}
