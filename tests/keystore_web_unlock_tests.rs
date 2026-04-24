#![cfg(all(feature = "server", feature = "keystore"))]
#![allow(clippy::await_holding_lock)]

use esdiag::{
    data::{authenticate, get_unlock_path, get_unlock_status, read_unlock_lease, write_unlock_lease},
    exporter::Exporter,
    server::{RuntimeMode, Server},
};
use reqwest::Client;
use serde::Deserialize;
use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tempfile::TempDir;
use tokio::time::sleep;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn setup_env() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("temp dir");
    let config_dir = tmp.path().join(".esdiag");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let keystore_path = config_dir.join("secrets.yml");
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("USERPROFILE", tmp.path());
        std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
        std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
    }
    (tmp, keystore_path)
}

fn run_esdiag(args: &[&str], home: &TempDir) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_esdiag"));
    cmd.args(args)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("ESDIAG_KEYSTORE", home.path().join(".esdiag").join("secrets.yml"))
        .env("LOG_LEVEL", "debug");
    cmd.output().expect("run esdiag")
}

async fn start_server(mode: RuntimeMode) -> (Server, Client, String) {
    let (server, bound_addr) = Server::start([127, 0, 0, 1], 0, Exporter::default(), String::new(), mode)
        .await
        .expect("start local server");

    let client = Client::new();
    let base = format!("http://127.0.0.1:{}", bound_addr.port());

    for _ in 0..40 {
        if client.get(format!("{base}/")).send().await.is_ok() {
            return (server, client, base);
        }
        sleep(Duration::from_millis(25)).await;
    }

    panic!("server did not become reachable in time");
}

#[derive(Deserialize)]
struct StatusResponse {
    locked: bool,
    expires_at_epoch: Option<i64>,
}

#[tokio::test]
async fn web_unlock_writes_unlock_file() {
    let _guard = env_lock().lock().expect("env lock");
    let (tmp, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("unlock request");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    let unlock_path = get_unlock_path().expect("unlock path");
    assert!(unlock_path.exists(), "unlock file should exist after web unlock");

    let status: StatusResponse = client
        .get(format!("{base}/keystore/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    assert!(!status.locked, "status endpoint should report unlocked");
    assert!(status.expires_at_epoch.is_some(), "status should include expiry");

    // Verify CLI sees the same state
    let cli_status = get_unlock_status().expect("cli unlock status");
    assert!(cli_status.unlock_active, "CLI should see unlock as active");

    let _ = tmp;
    server.shutdown().await;
}

#[tokio::test]
async fn web_lock_deletes_unlock_file() {
    let _guard = env_lock().lock().expect("env lock");
    let (tmp, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    // Unlock first
    let response = client
        .post(format!("{base}/keystore/unlock"))
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("unlock request");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    let unlock_path = get_unlock_path().expect("unlock path");
    assert!(unlock_path.exists(), "unlock file should exist");

    // Now lock
    let response = client
        .post(format!("{base}/keystore/lock"))
        .send()
        .await
        .expect("lock request");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    assert!(!unlock_path.exists(), "unlock file should be deleted after web lock");

    let status: StatusResponse = client
        .get(format!("{base}/keystore/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    assert!(status.locked, "status endpoint should report locked");
    assert!(status.expires_at_epoch.is_none());

    let _ = tmp;
    server.shutdown().await;
}

#[tokio::test]
async fn cli_unlock_reflected_in_web_status() {
    let _guard = env_lock().lock().expect("env lock");
    let (tmp, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    // Write unlock lease directly (simulates CLI unlock)
    write_unlock_lease("pw", Duration::from_secs(3600)).expect("write unlock lease");

    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let status: StatusResponse = client
        .get(format!("{base}/keystore/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    assert!(!status.locked, "web should reflect CLI unlock as unlocked");
    assert!(status.expires_at_epoch.is_some());

    let _ = tmp;
    server.shutdown().await;
}

#[tokio::test]
async fn cli_lock_reflected_in_web_status() {
    let _guard = env_lock().lock().expect("env lock");
    let (tmp, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    // Write unlock lease, then delete it via CLI
    write_unlock_lease("pw", Duration::from_secs(3600)).expect("write unlock lease");

    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    // Confirm unlocked
    let status: StatusResponse = client
        .get(format!("{base}/keystore/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    assert!(!status.locked, "should start unlocked");

    // Lock via CLI
    let lock_output = run_esdiag(&["keystore", "lock"], &tmp);
    assert!(lock_output.status.success(), "CLI lock should succeed");

    // Web should now show locked
    let status: StatusResponse = client
        .get(format!("{base}/keystore/status"))
        .send()
        .await
        .expect("status after CLI lock")
        .json()
        .await
        .expect("status json");
    assert!(status.locked, "web should reflect CLI lock");

    let _ = tmp;
    server.shutdown().await;
}

#[tokio::test]
async fn web_unlock_status_verified_by_cli() {
    let _guard = env_lock().lock().expect("env lock");
    let (tmp, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    let (mut server, client, base) = start_server(RuntimeMode::User).await;

    let response = client
        .post(format!("{base}/keystore/unlock"))
        .form(&[("password", "pw")])
        .send()
        .await
        .expect("unlock request");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    // Verify via library helper
    let status = get_unlock_status().expect("cli unlock status");
    assert!(status.unlock_active, "CLI should report unlock active");
    assert!(status.expires_at_epoch.is_some(), "CLI should report expiry");

    // Verify via file
    let lease = read_unlock_lease().expect("read lease").expect("lease should exist");
    let now = chrono::Utc::now().timestamp();
    assert!(lease.expires_at_epoch > now, "lease should expire in the future");

    let _ = tmp;
    server.shutdown().await;
}
