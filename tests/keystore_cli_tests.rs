use esdiag::data::{Auth, HostRole, KnownHost, Product, authenticate, get_secret, get_unlock_status};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command, Output},
    sync::{Mutex, OnceLock},
};
use tempfile::TempDir;
use url::Url;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn setup_env() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().expect("temp dir");
    let config_dir = tmp.path().join(".esdiag");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let hosts_path = config_dir.join("hosts.yml");
    let keystore_path = config_dir.join("secrets.yml");
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("USERPROFILE", tmp.path());
        std::env::set_var("ESDIAG_HOSTS", &hosts_path);
        std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
    }
    (tmp, hosts_path, keystore_path)
}

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
fn keystore_add_command_stores_secret_that_reads_back_with_values() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, keystore_path) = setup_env();

    let add_output = run_esdiag(
        &[
            "keystore",
            "add",
            "cli-basic",
            "--user",
            "elastic",
            "--password",
            "pass-1",
        ],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&add_output, "keystore add");

    let secret = get_secret("cli-basic", "pw")
        .expect("read secret")
        .expect("secret should exist");
    let raw_keystore = std::fs::read_to_string(&keystore_path).expect("read keystore");

    assert_eq!(
        secret.basic.as_ref().map(|b| b.username.as_str()),
        Some("elastic")
    );
    assert_eq!(
        secret.basic.as_ref().map(|b| b.password.as_str()),
        Some("pass-1")
    );
    assert!(secret.apikey.is_none());
    assert!(!raw_keystore.contains("elastic"));
    assert!(!raw_keystore.contains("pass-1"));
}

#[test]
fn keystore_migrate_command_scrubs_plaintext_hosts_and_preserves_reads() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, hosts_path, _keystore_path) = setup_env();
    unsafe {
        std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
    }

    let mut hosts = BTreeMap::new();
    hosts.insert(
        "es-prod".to_string(),
        KnownHost::ApiKey {
            accept_invalid_certs: false,
            apikey: Some("apikey-1".to_string()),
            app: Product::Elasticsearch,
            cloud_id: None,
            roles: vec![HostRole::Collect],
            secret: None,
            viewer: None,
            url: Url::parse("http://localhost:9200").expect("url"),
        },
    );
    hosts.insert(
        "kb-prod".to_string(),
        KnownHost::Basic {
            accept_invalid_certs: false,
            app: Product::Kibana,
            password: Some("pass-1".to_string()),
            roles: vec![HostRole::Collect],
            secret: None,
            viewer: None,
            url: Url::parse("http://localhost:5601").expect("url"),
            username: Some("elastic".to_string()),
        },
    );
    KnownHost::write_hosts_yml(&hosts).expect("write plaintext hosts");

    let migrate_output = run_esdiag(
        &["keystore", "migrate"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&migrate_output, "keystore migrate");

    let migrated_hosts = KnownHost::parse_hosts_yml().expect("read migrated hosts");
    let raw_hosts = std::fs::read_to_string(&hosts_path).expect("read hosts");

    match migrated_hosts.get("es-prod").expect("es host exists") {
        KnownHost::ApiKey { apikey, secret, .. } => {
            assert!(apikey.is_none(), "plaintext api key should be removed");
            assert_eq!(secret.as_deref(), Some("es-prod"));
        }
        _ => panic!("expected api key host"),
    }

    match migrated_hosts.get("kb-prod").expect("kb host exists") {
        KnownHost::Basic {
            username,
            password,
            secret,
            ..
        } => {
            assert!(username.is_none(), "plaintext username should be removed");
            assert!(password.is_none(), "plaintext password should be removed");
            assert_eq!(secret.as_deref(), Some("kb-prod"));
        }
        _ => panic!("expected basic host"),
    }

    assert!(!raw_hosts.contains("apikey: apikey-1"));
    assert!(!raw_hosts.contains("username: elastic"));
    assert!(!raw_hosts.contains("password: pass-1"));

    let es_secret = get_secret("es-prod", "pw")
        .expect("read es secret")
        .expect("es secret exists");
    assert_eq!(es_secret.apikey.as_deref(), Some("apikey-1"));

    let kb_secret = get_secret("kb-prod", "pw")
        .expect("read kb secret")
        .expect("kb secret exists");
    assert_eq!(
        kb_secret.basic.as_ref().map(|b| b.username.as_str()),
        Some("elastic")
    );
    assert_eq!(
        kb_secret.basic.as_ref().map(|b| b.password.as_str()),
        Some("pass-1")
    );

    let es_auth = migrated_hosts
        .get("es-prod")
        .expect("es host exists")
        .get_auth()
        .expect("resolve es auth");
    assert!(matches!(es_auth, Auth::Apikey(key) if key == "apikey-1"));

    let kb_auth = migrated_hosts
        .get("kb-prod")
        .expect("kb host exists")
        .get_auth()
        .expect("resolve kb auth");
    assert!(matches!(kb_auth, Auth::Basic(user, pass) if user == "elastic" && pass == "pass-1"));
}

#[test]
fn keystore_add_rejects_duplicate_secret() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, _keystore_path) = setup_env();

    let first_add = run_esdiag(
        &["keystore", "add", "prod-es", "--apikey", "key-1"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&first_add, "first keystore add");

    let second_add = run_esdiag(
        &["keystore", "add", "prod-es", "--apikey", "key-2"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert!(
        !second_add.status.success(),
        "duplicate add should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_add.stdout),
        String::from_utf8_lossy(&second_add.stderr)
    );
    assert!(
        String::from_utf8_lossy(&second_add.stderr).contains("already exists"),
        "expected duplicate-secret error"
    );

    let secret = get_secret("prod-es", "pw")
        .expect("read secret")
        .expect("secret should still exist");
    assert_eq!(secret.apikey.as_deref(), Some("key-1"));
}

#[test]
fn keystore_update_updates_existing_secret_and_rejects_missing_secret() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, _keystore_path) = setup_env();

    let missing_update = run_esdiag(
        &["keystore", "update", "missing", "--apikey", "key-1"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert!(
        !missing_update.status.success(),
        "missing update should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&missing_update.stdout),
        String::from_utf8_lossy(&missing_update.stderr)
    );
    assert!(
        String::from_utf8_lossy(&missing_update.stderr).contains("not found"),
        "expected missing-secret error"
    );

    let add_output = run_esdiag(
        &["keystore", "add", "prod-es", "--apikey", "key-1"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&add_output, "keystore add before update");

    let update_output = run_esdiag(
        &[
            "keystore",
            "update",
            "prod-es",
            "--user",
            "elastic",
            "--password",
            "pass-2",
        ],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&update_output, "keystore update");

    let secret = get_secret("prod-es", "pw")
        .expect("read updated secret")
        .expect("updated secret exists");
    assert_eq!(
        secret.basic.as_ref().map(|basic| basic.username.as_str()),
        Some("elastic")
    );
    assert_eq!(
        secret.basic.as_ref().map(|basic| basic.password.as_str()),
        Some("pass-2")
    );
    assert!(secret.apikey.is_none(), "update should replace previous auth type");
}

#[test]
fn keystore_unlock_status_and_lock_commands_manage_unlock_file() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, _keystore_path) = setup_env();
    authenticate("pw").expect("create keystore");

    let unlock_output = run_esdiag(
        &["keystore", "unlock", "--ttl", "7d"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert_success(&unlock_output, "keystore unlock");

    let unlock_path = home.path().join(".esdiag").join("keystore.unlock");
    assert!(unlock_path.exists(), "unlock file should exist");

    let status_output = run_esdiag(&["keystore", "status"], &home, &[]);
    assert_success(&status_output, "keystore status while unlocked");
    let status = get_unlock_status().expect("unlock status while unlocked");
    assert!(status.unlock_active, "status helper should report active unlock");
    assert!(status.expires_at_epoch.is_some(), "status should include expiry");

    let lock_output = run_esdiag(&["keystore", "lock"], &home, &[]);
    assert_success(&lock_output, "keystore lock");
    assert!(!unlock_path.exists(), "unlock file should be removed");

    let status_after_lock = run_esdiag(&["keystore", "status"], &home, &[]);
    assert_success(&status_after_lock, "keystore status after lock");
    let status_after_lock = get_unlock_status().expect("unlock status after lock");
    assert!(
        !status_after_lock.unlock_active,
        "status helper should report inactive unlock"
    );
}

#[test]
fn keystore_unlock_refuses_noninteractive_bootstrap_without_keystore() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, _keystore_path) = setup_env();

    let output = run_esdiag(&["keystore", "unlock"], &home, &[]);
    assert!(
        !output.status.success(),
        "unlock without keystore should fail non-interactively\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No keystore exists"),
        "expected missing-keystore error"
    );
}

#[test]
fn keystore_add_missing_secret_material_fails_noninteractive() {
    let _guard = env_lock().lock().expect("env lock");
    let (home, _hosts_path, _keystore_path) = setup_env();

    let output = run_esdiag(
        &["keystore", "add", "prod-es", "--apikey"],
        &home,
        &[("ESDIAG_KEYSTORE_PASSWORD", "pw")],
    );
    assert!(
        !output.status.success(),
        "missing secret material should fail without tty\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Required secret value was not provided"),
        "expected missing secret value error"
    );
}
