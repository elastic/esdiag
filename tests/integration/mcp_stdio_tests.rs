// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(60);

fn send(stdin: &mut impl Write, message: Value) {
    writeln!(stdin, "{message}").expect("write request");
    stdin.flush().expect("flush request");
}

#[test]
fn stdio_session_carries_only_protocol_messages_and_exits_cleanly() {
    let home = TempDir::new().expect("temp dir");
    let config = home.path().join(".esdiag");
    let output = home.path().join("processed");
    std::fs::create_dir_all(&config).expect("config dir");
    std::fs::create_dir_all(&output).expect("output dir");
    let archive = format!(
        "{}/tests/archives/logstash-api-diagnostics-9.3.3.zip",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["--debug", "mcp"])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_HOSTS", config.join("hosts.yml"))
        .env("ESDIAG_KEYSTORE", config.join("secrets.yml"))
        .env("ESDIAG_SETTINGS", config.join("settings.yml"))
        .env_remove("ESDIAG_KEYSTORE_PASSWORD")
        .env_remove("LOG_LEVEL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn esdiag mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let (lines_tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let mut seen = Vec::new();
    let mut wait_for = |id: u64| loop {
        let line = lines.recv_timeout(TIMEOUT).expect("server response");
        let message: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("non-JSON stdout line: {line}"));
        assert_eq!(message["jsonrpc"], "2.0", "{line}");
        let done = message["id"] == id;
        seen.push(message);
        if done {
            return seen.last().cloned().expect("response");
        }
    };

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "esdiag-test", "version": "0" }
            }
        }),
    );
    let initialized = wait_for(1);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "esdiag");
    assert_eq!(
        initialized["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(initialized["result"]["capabilities"]["tools"].is_object());
    send(
        &mut stdin,
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    );

    send(&mut stdin, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = wait_for(2);
    let names = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    for name in ["host_list", "process", "collect", "send", "host_add", "host_remove"] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }

    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {
                "name": "process",
                "arguments": { "input": archive, "output": output.display().to_string() },
                "_meta": { "progressToken": "process-1" }
            }
        }),
    );
    let processed = wait_for(3);
    assert_eq!(processed["result"]["isError"], false, "{processed}");
    assert_eq!(
        processed["result"]["structuredContent"]["result"],
        "diagnostic_processed"
    );
    let progress = seen
        .iter()
        .filter(|message| message["method"] == "notifications/progress")
        .filter(|message| message["params"]["progressToken"] == "process-1")
        .count();
    assert!(progress > 0, "no progress before the process result");

    drop(stdin);
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll esdiag mcp") {
            break status;
        }
        assert!(
            started.elapsed() < TIMEOUT,
            "esdiag mcp did not exit after stdin closed"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = stderr_reader.join().expect("stderr");
    assert!(status.success(), "exit status {status:?}\nstderr:\n{stderr}");
    assert!(stderr.contains("DEBUG"), "debug logs belong on stderr:\n{stderr}");
}
