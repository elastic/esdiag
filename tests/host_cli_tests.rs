use axum::{Json, Router, routing::get};
use esdiag::data::{HostRole, KnownHost, Settings};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command, Output},
    time::Duration,
};
use tempfile::TempDir;

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

async fn run_esdiag_async(args: Vec<String>, home: PathBuf, extra_env: Vec<(String, String)>) -> Output {
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
        cmd.args(&args)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("ESDIAG_HOSTS", home.join(".esdiag").join("hosts.yml"))
            .env("ESDIAG_KEYSTORE", home.join(".esdiag").join("secrets.yml"))
            .env("LOG_LEVEL", "debug");
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        cmd.output().expect("run esdiag")
    })
    .await
    .expect("join esdiag process")
}

fn setup_home() -> TempDir {
    let home = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(home.path().join(".esdiag")).expect("create config dir");
    home
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

fn assert_failure_contains(output: &Output, expected: &str, context: &str) {
    assert!(
        !output.status.success(),
        "{context} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "{context} stderr missing expected text `{expected}`\nstderr:\n{stderr}"
    );
}

fn assert_stdout_contains(output: &Output, expected: &str, context: &str) {
    assert_success(output, context);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(expected),
        "{context} stdout missing expected text `{expected}`\nstdout:\n{stdout}"
    );
}

fn assert_stderr_contains(output: &Output, expected: &str, context: &str) {
    assert_success(output, context);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "{context} stderr missing expected text `{expected}`\nstderr:\n{stderr}"
    );
}

fn read_hosts(home: &TempDir) -> BTreeMap<String, KnownHost> {
    let path = home.path().join(".esdiag").join("hosts.yml");
    if !path.exists() {
        return BTreeMap::new();
    }
    let content = std::fs::read_to_string(path).expect("read hosts");
    if content.trim().is_empty() {
        return BTreeMap::new();
    }
    serde_yaml::from_str(&content).expect("parse hosts yaml")
}

fn read_settings(home: &TempDir) -> Settings {
    let path = home.path().join(".esdiag").join("settings.yml");
    if !path.exists() {
        return Settings::default();
    }
    let content = std::fs::read_to_string(path).expect("read settings");
    serde_yaml::from_str(&content).expect("parse settings")
}

fn write_settings(home: &TempDir, settings: &Settings) {
    let path = home.path().join(".esdiag").join("settings.yml");
    let content = serde_yaml::to_string(settings).expect("serialize settings");
    std::fs::write(path, content).expect("write settings");
}

async fn start_mock_elasticsearch() -> (String, tokio::sync::oneshot::Sender<()>) {
    async fn root_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "tagline": "You Know, for Search"
        }))
    }

    let app = Router::new().route("/", get(root_handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock elasticsearch");
    let addr = listener.local_addr().expect("listener addr");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve mock elasticsearch");
    });

    let url = format!("http://{addr}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = reqwest::get(&url).await
            && response.status().is_success()
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "mock elasticsearch did not become ready in time"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    (url, shutdown_tx)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_preserves_omitted_fields_and_applies_cert_overrides() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();
    let secret_env = vec![("ESDIAG_KEYSTORE_PASSWORD".to_string(), "pw".to_string())];

    let add_secret = run_esdiag_async(
        vec![
            "keystore".to_string(),
            "add".to_string(),
            "prod-secret".to_string(),
            "--apikey".to_string(),
            "legacy-key".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&add_secret, "add host secret");

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
            "--secret".to_string(),
            "prod-secret".to_string(),
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&create, "create host");

    let update_roles = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--roles".to_string(),
            "collect,send".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&update_roles, "update roles");

    let hosts = read_hosts(&home);
    let host = hosts.get("prod-es").expect("saved host exists");
    assert!(host.accept_invalid_certs, "omitted cert flag should preserve value");
    assert!(host.legacy_apikey.is_none());
    assert_eq!(host.secret.as_deref(), Some("prod-secret"));
    assert_eq!(host.roles, vec![HostRole::Collect, HostRole::Send]);

    let disable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "false".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&disable_certs, "disable accept invalid certs");

    let hosts = read_hosts(&home);
    assert!(
        !hosts.get("prod-es").expect("saved host exists").accept_invalid_certs(),
        "explicit false should clear accept_invalid_certs"
    );

    let enable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path,
        secret_env,
    )
    .await;
    assert_success(&enable_certs, "enable accept invalid certs");

    let hosts = read_hosts(&home);
    assert!(
        hosts.get("prod-es").expect("saved host exists").accept_invalid_certs(),
        "explicit true should set accept_invalid_certs"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_applies_cert_overrides_for_noauth_hosts() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "plain-es".to_string(),
            "elasticsearch".to_string(),
            url,
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "create noauth host");

    let disable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "plain-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "false".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&disable_certs, "disable noauth cert override");

    let hosts = read_hosts(&home);
    assert!(
        !hosts.get("plain-es").expect("saved host exists").accept_invalid_certs,
        "explicit false should clear accept_invalid_certs for noauth hosts"
    );

    let enable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "plain-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_success(&enable_certs, "enable noauth cert override");

    let hosts = read_hosts(&home);
    assert!(
        hosts.get("plain-es").expect("saved host exists").accept_invalid_certs,
        "explicit true should set accept_invalid_certs for noauth hosts"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_supports_secret_rotation_and_rejects_persisted_apikey_override() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();
    let secret_env = vec![("ESDIAG_KEYSTORE_PASSWORD".to_string(), "pw".to_string())];

    let add_old = run_esdiag_async(
        vec![
            "keystore".to_string(),
            "add".to_string(),
            "old-secret".to_string(),
            "--apikey".to_string(),
            "old-key".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&add_old, "add old secret");

    let add_new = run_esdiag_async(
        vec![
            "keystore".to_string(),
            "add".to_string(),
            "new-secret".to_string(),
            "--apikey".to_string(),
            "new-secret-key".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&add_new, "add new secret");

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
            "--secret".to_string(),
            "old-secret".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&create, "create secret-backed host");

    let rotate_secret = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--secret".to_string(),
            "new-secret".to_string(),
        ],
        home_path.clone(),
        secret_env.clone(),
    )
    .await;
    assert_success(&rotate_secret, "rotate secret");

    let hosts = read_hosts(&home);
    let host = hosts.get("prod-es").expect("saved host exists");
    assert!(
        host.legacy_apikey.is_none(),
        "secret-backed host should not persist api key"
    );
    assert_eq!(host.secret.as_deref(), Some("new-secret"));

    let override_apikey = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--apikey".to_string(),
            "override-key".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_failure_contains(
        &override_apikey,
        "requires a secret reference before it can be saved",
        "persisted apikey override",
    );

    let hosts = read_hosts(&home);
    let host = hosts.get("prod-es").expect("saved host exists");
    assert!(host.legacy_apikey.is_none());
    assert_eq!(host.secret.as_deref(), Some("new-secret"));

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_remove_deletes_saved_host_and_updates_settings() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create_prod = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create_prod, "create prod host");

    let create_other = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "other-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create_other, "create other host");

    write_settings(
        &home,
        &Settings {
            active_target: Some("prod-es".to_string()),
            kibana_url: None,
        },
    );

    let delete = run_esdiag_async(
        vec!["host".to_string(), "remove".to_string(), "prod-es".to_string()],
        home_path,
        vec![],
    )
    .await;
    assert_success(&delete, "delete host");

    let hosts = read_hosts(&home);
    assert!(!hosts.contains_key("prod-es"));
    assert!(hosts.contains_key("other-es"));

    let settings = read_settings(&home);
    assert_eq!(settings.active_target.as_deref(), Some("other-es"));

    let _ = shutdown_tx.send(());
}

#[test]
fn host_add_guardrails_and_missing_host_updates_fail() {
    let home = setup_home();

    let incomplete_add = run_esdiag(&["host", "add", "prod-es", "--secret", "rotated"], &home, &[]);
    assert_failure_contains(
        &incomplete_add,
        "required arguments were not provided",
        "incomplete add",
    );

    let missing_update = run_esdiag(&["host", "update", "missing-es", "--secret", "rotated"], &home, &[]);
    assert_failure_contains(&missing_update, "Host 'missing-es' not found", "missing host update");

    let missing_delete = run_esdiag(&["host", "remove", "missing-es"], &home, &[]);
    assert_failure_contains(&missing_delete, "Host 'missing-es' not found", "missing delete");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_rejects_partial_basic_auth_without_secret() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "plain-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "create noauth host");

    let update = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "plain-es".to_string(),
            "--user".to_string(),
            "elastic".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_failure_contains(
        &update,
        "either provide a secret reference or both username and password",
        "partial basic auth update",
    );

    let hosts = read_hosts(&home);
    assert!(
        hosts.get("plain-es").is_some_and(|host| host.secret.is_none()
            && host.legacy_apikey.is_none()
            && host.legacy_username.is_none()
            && host.legacy_password.is_none()),
        "failed update should leave the saved host unchanged"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_rejects_partial_basic_auth_for_existing_basic_host() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();
    let hosts_path = home.path().join(".esdiag").join("hosts.yml");
    let content = format!(
        concat!(
            "basic-es:\n",
            "  auth: Basic\n",
            "  app: Elasticsearch\n",
            "  username: elastic\n",
            "  password: old-pass\n",
            "  roles:\n",
            "    - collect\n",
            "  url: {url}\n",
        ),
        url = url
    );
    std::fs::write(&hosts_path, content).expect("write hosts");

    let update = run_esdiag_async(
        vec![
            "host".to_string(),
            "update".to_string(),
            "basic-es".to_string(),
            "--user".to_string(),
            "new-user".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_failure_contains(
        &update,
        "either provide a secret reference or both username and password",
        "partial basic auth update for existing basic host",
    );

    let hosts = read_hosts(&home);
    let host = hosts.get("basic-es").expect("saved host exists");
    assert_eq!(host.legacy_username.as_deref(), Some("elastic"));
    assert_eq!(host.legacy_password.as_deref(), Some("old-pass"));
    assert!(host.secret.is_none(), "failed update should not add a secret reference");

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_remove_succeeds_even_if_settings_cleanup_fails() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "create host");

    write_settings(
        &home,
        &Settings {
            active_target: Some("prod-es".to_string()),
            kibana_url: None,
        },
    );
    std::fs::write(
        home.path().join(".esdiag").join("settings.yml"),
        "active_target: [broken\n",
    )
    .expect("write invalid settings");

    let delete = run_esdiag_async(
        vec!["host".to_string(), "remove".to_string(), "prod-es".to_string()],
        home_path,
        vec![],
    )
    .await;
    assert_success(&delete, "delete host with invalid settings");
    assert!(
        String::from_utf8_lossy(&delete.stderr).contains("failed to update settings"),
        "expected settings cleanup warning, stderr was:\n{}",
        String::from_utf8_lossy(&delete.stderr)
    );

    let hosts = read_hosts(&home);
    assert!(!hosts.contains_key("prod-es"));

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_add_rejects_duplicate_names() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "initial add");

    let duplicate = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path,
        vec![],
    )
    .await;
    assert_failure_contains(&duplicate, "already exists", "duplicate add");

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_list_prints_empty_state_and_saved_rows() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let empty_list = run_esdiag(&["host", "list"], &home, &[]);
    assert_stdout_contains(&empty_list, "No saved hosts", "empty host list");
    let empty_stderr = String::from_utf8_lossy(&empty_list.stderr);
    assert!(
        !empty_stderr.contains("host list complete"),
        "host list should not emit completion summary:\n{empty_stderr}"
    );

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path,
        vec![],
    )
    .await;
    assert_success(&create, "add host for list");

    let populated_list = run_esdiag(&["host", "list"], &home, &[]);
    assert_success(&populated_list, "populated host list");
    let stdout = String::from_utf8_lossy(&populated_list.stdout);
    let stderr = String::from_utf8_lossy(&populated_list.stderr);
    assert!(stdout.contains("name"), "expected header in list output:\n{stdout}");
    assert!(stdout.contains("app"), "expected header in list output:\n{stdout}");
    assert!(stdout.contains("secret"), "expected header in list output:\n{stdout}");
    assert!(
        stdout.contains("prod-es"),
        "expected saved host row in list output:\n{stdout}"
    );
    assert!(
        !stderr.contains("host list complete"),
        "host list should not emit completion summary:\n{stderr}"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_auth_tests_saved_host_without_mutating_it() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url,
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "add host for auth");
    let hosts_path = home.path().join(".esdiag").join("hosts.yml");
    let before = std::fs::read_to_string(&hosts_path).expect("read hosts before auth");

    let auth = run_esdiag_async(
        vec!["host".to_string(), "auth".to_string(), "prod-es".to_string()],
        home_path,
        vec![],
    )
    .await;
    assert_success(&auth, "auth host");
    let auth_stderr = String::from_utf8_lossy(&auth.stderr);
    assert!(
        auth_stderr.contains("Host prod-es: 200 OK"),
        "auth should emit a meaningful connection summary:\n{auth_stderr}"
    );
    assert!(
        !auth_stderr.contains("host auth complete"),
        "auth should not fall back to a generic completion summary:\n{auth_stderr}"
    );

    let after = std::fs::read_to_string(&hosts_path).expect("read hosts after auth");
    assert_eq!(before, after, "auth should not mutate saved hosts");

    let _ = shutdown_tx.send(());
}

#[test]
fn host_auth_missing_host_and_legacy_syntax_fail() {
    let home = setup_home();

    let missing_auth = run_esdiag(&["host", "auth", "missing-es"], &home, &[]);
    assert_failure_contains(&missing_auth, "Host 'missing-es' not found", "missing auth host");

    let legacy = run_esdiag(&["host", "prod-es", "--secret", "rotated"], &home, &[]);
    assert_failure_contains(
        &legacy,
        "Legacy positional host syntax is no longer supported",
        "legacy host syntax",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_subcommands_emit_meaningful_agent_summaries() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let add = run_esdiag_async(
        vec![
            "--agent".to_string(),
            "host".to_string(),
            "add".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_stderr_contains(&add, "Host prod-es: 200 OK", "agent host add");
    assert_stderr_contains(&add, "Host 'prod-es' added in", "agent host add");
    assert!(
        !String::from_utf8_lossy(&add.stderr).contains("host add complete"),
        "agent host add should not emit a generic completion summary:\n{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let update = run_esdiag_async(
        vec![
            "--agent".to_string(),
            "host".to_string(),
            "update".to_string(),
            "prod-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "false".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_stderr_contains(&update, "Host prod-es: 200 OK", "agent host update");
    assert_stderr_contains(&update, "Host 'prod-es' updated in", "agent host update");
    assert!(
        !String::from_utf8_lossy(&update.stderr).contains("host update complete"),
        "agent host update should not emit a generic completion summary:\n{}",
        String::from_utf8_lossy(&update.stderr)
    );

    let auth = run_esdiag_async(
        vec![
            "--agent".to_string(),
            "host".to_string(),
            "auth".to_string(),
            "prod-es".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_stderr_contains(&auth, "Host prod-es: 200 OK", "agent host auth");
    assert!(
        !String::from_utf8_lossy(&auth.stderr).contains("host auth complete"),
        "agent host auth should not emit a generic completion summary:\n{}",
        String::from_utf8_lossy(&auth.stderr)
    );

    let remove = run_esdiag_async(
        vec![
            "--agent".to_string(),
            "host".to_string(),
            "remove".to_string(),
            "prod-es".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_stderr_contains(&remove, "Host 'prod-es' removed from", "agent host remove");
    assert!(
        !String::from_utf8_lossy(&remove.stderr).contains("host remove complete"),
        "agent host remove should not emit a generic completion summary:\n{}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let _ = shutdown_tx.send(());
}
