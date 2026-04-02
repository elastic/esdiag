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
        .env(
            "ESDIAG_HOSTS",
            home.path().join(".esdiag").join("hosts.yml"),
        )
        .env(
            "ESDIAG_KEYSTORE",
            home.path().join(".esdiag").join("secrets.yml"),
        )
        .env("LOG_LEVEL", "debug");
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    cmd.output().expect("run esdiag")
}

async fn run_esdiag_async(
    args: Vec<String>,
    home: PathBuf,
    extra_env: Vec<(String, String)>,
) -> Output {
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

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "elasticsearch".to_string(),
            url.clone(),
            "--apikey".to_string(),
            "legacy-key".to_string(),
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "create host");

    let update_roles = run_esdiag_async(
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--roles".to_string(),
            "collect,send".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&update_roles, "update roles");

    let hosts = read_hosts(&home);
    match hosts.get("prod-es").expect("saved host exists") {
        KnownHost::ApiKey {
            accept_invalid_certs,
            apikey,
            roles,
            secret,
            ..
        } => {
            assert!(
                *accept_invalid_certs,
                "omitted cert flag should preserve value"
            );
            assert_eq!(apikey.as_deref(), Some("legacy-key"));
            assert!(secret.is_none());
            assert_eq!(roles, &vec![HostRole::Collect, HostRole::Send]);
        }
        _ => panic!("expected api key host"),
    }

    let disable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "false".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&disable_certs, "disable accept invalid certs");

    let hosts = read_hosts(&home);
    assert!(
        !hosts
            .get("prod-es")
            .expect("saved host exists")
            .accept_invalid_certs(),
        "explicit false should clear accept_invalid_certs"
    );

    let enable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--accept-invalid-certs".to_string(),
            "true".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_success(&enable_certs, "enable accept invalid certs");

    let hosts = read_hosts(&home);
    assert!(
        hosts
            .get("prod-es")
            .expect("saved host exists")
            .accept_invalid_certs(),
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
    match hosts.get("plain-es").expect("saved host exists") {
        KnownHost::NoAuth {
            accept_invalid_certs,
            ..
        } => assert!(
            !accept_invalid_certs,
            "explicit false should clear accept_invalid_certs for noauth hosts"
        ),
        _ => panic!("expected noauth host"),
    }

    let enable_certs = run_esdiag_async(
        vec![
            "host".to_string(),
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
    match hosts.get("plain-es").expect("saved host exists") {
        KnownHost::NoAuth {
            accept_invalid_certs,
            ..
        } => assert!(
            *accept_invalid_certs,
            "explicit true should set accept_invalid_certs for noauth hosts"
        ),
        _ => panic!("expected noauth host"),
    }

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_supports_secret_rotation_and_apikey_override() {
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
    match hosts.get("prod-es").expect("saved host exists") {
        KnownHost::ApiKey { apikey, secret, .. } => {
            assert!(
                apikey.is_none(),
                "secret-backed host should not persist api key"
            );
            assert_eq!(secret.as_deref(), Some("new-secret"));
        }
        _ => panic!("expected api key host"),
    }

    let override_apikey = run_esdiag_async(
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--apikey".to_string(),
            "override-key".to_string(),
        ],
        home_path,
        vec![],
    )
    .await;
    assert_success(&override_apikey, "override apikey");

    let hosts = read_hosts(&home);
    match hosts.get("prod-es").expect("saved host exists") {
        KnownHost::ApiKey { apikey, secret, .. } => {
            assert_eq!(apikey.as_deref(), Some("override-key"));
            assert!(
                secret.is_none(),
                "apikey override should clear secret reference"
            );
        }
        _ => panic!("expected api key host"),
    }

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_delete_removes_saved_host_and_updates_settings() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create_prod = run_esdiag_async(
        vec![
            "host".to_string(),
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
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--delete".to_string(),
        ],
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
fn host_delete_conflicts_and_missing_host_updates_fail() {
    let home = setup_home();

    let delete_conflict = run_esdiag(
        &["host", "prod-es", "--delete", "--secret", "rotated"],
        &home,
        &[],
    );
    assert_failure_contains(&delete_conflict, "--delete", "delete conflict");

    let missing_update = run_esdiag(&["host", "missing-es", "--secret", "rotated"], &home, &[]);
    assert_failure_contains(
        &missing_update,
        "Host missing-es not found",
        "missing host update",
    );

    let missing_delete = run_esdiag(&["host", "missing-es", "--delete"], &home, &[]);
    assert_failure_contains(
        &missing_delete,
        "Host 'missing-es' not found",
        "missing delete",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_rejects_partial_basic_auth_without_secret() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
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
        matches!(hosts.get("plain-es"), Some(KnownHost::NoAuth { .. })),
        "failed update should leave the saved host unchanged"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_update_rejects_partial_basic_auth_for_existing_basic_host() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
            "basic-es".to_string(),
            "elasticsearch".to_string(),
            url,
            "--user".to_string(),
            "elastic".to_string(),
            "--password".to_string(),
            "old-pass".to_string(),
        ],
        home_path.clone(),
        vec![],
    )
    .await;
    assert_success(&create, "create basic host");

    let update = run_esdiag_async(
        vec![
            "host".to_string(),
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
    match hosts.get("basic-es").expect("saved host exists") {
        KnownHost::Basic {
            username,
            password,
            secret,
            ..
        } => {
            assert_eq!(username.as_deref(), Some("elastic"));
            assert_eq!(password.as_deref(), Some("old-pass"));
            assert!(
                secret.is_none(),
                "failed update should not add a secret reference"
            );
        }
        _ => panic!("expected basic host"),
    }

    let _ = shutdown_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_delete_succeeds_even_if_settings_cleanup_fails() {
    let (url, shutdown_tx) = start_mock_elasticsearch().await;
    let home = setup_home();
    let home_path = home.path().to_path_buf();

    let create = run_esdiag_async(
        vec![
            "host".to_string(),
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
        vec![
            "host".to_string(),
            "prod-es".to_string(),
            "--delete".to_string(),
        ],
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
