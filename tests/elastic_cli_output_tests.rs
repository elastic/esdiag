// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use std::{fs, path::Path, process::Command};

fn command(home: &Path) -> Command {
    let state_dir = home.join(".esdiag");
    fs::create_dir_all(&state_dir).expect("create state directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    command
        .env("ESDIAG_ELASTIC_CLI", "1")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("ESDIAG_HOSTS", state_dir.join("hosts.yml"))
        .env("ELASTIC_CLI_CONFIG_FILE", home.join(".elasticrc.yml"));
    command
}

#[test]
fn output_context_set_show_and_clear_persist_only_symbolic_state() {
    let home = tempfile::TempDir::new().expect("temp home");
    fs::write(
        home.path().join(".elasticrc.yml"),
        "current_context: prod\ncontexts:\n  monitoring:\n    elasticsearch:\n      url: https://monitoring.example:9200\n      auth:\n        api_key: do-not-persist\n",
    )
    .expect("write elasticrc");

    let set = command(home.path())
        .args(["--format", "json", "output", "set", "monitoring"])
        .output()
        .expect("set output");
    assert!(
        set.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let saved = fs::read_to_string(home.path().join(".esdiag/esdiag.yml")).expect("read app config");
    assert!(saved.contains("context: monitoring"));
    assert!(saved.contains("config_file:"));
    assert!(!saved.contains("do-not-persist"));
    assert!(!saved.contains("monitoring.example"));

    let show = command(home.path())
        .args(["--format", "json", "output", "show"])
        .output()
        .expect("show output");
    assert!(show.status.success());
    let shown = String::from_utf8_lossy(&show.stdout);
    assert!(shown.contains("\"operation\":\"show\""));
    assert!(shown.contains("\"context\":\"monitoring\""));

    let clear = command(home.path())
        .args(["--format", "json", "output", "clear"])
        .output()
        .expect("clear output");
    assert!(clear.status.success());
    let cleared = fs::read_to_string(home.path().join(".esdiag/esdiag.yml")).expect("read cleared config");
    assert!(!cleared.contains("elastic_context"));
    assert!(!cleared.contains("monitoring"));
}
