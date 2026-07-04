use std::{
    env,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

// These tests intentionally remain ignored because OS keychain access may prompt
// for interactive user approval and is not suitable for CI or unattended runs.

fn elastic_path() -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|dir| dir.join("elastic"))
            .find(|candidate| candidate.is_file())
    })
}

fn handle_mock_request(mut stream: TcpStream, auth_headers: &Arc<Mutex<Vec<String>>>) {
    let mut buffer = [0; 8192];
    let Ok(size) = stream.read(&mut buffer) else {
        return;
    };
    let request = String::from_utf8_lossy(&buffer[..size]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let auth = request
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization").then_some(value)
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    auth_headers.lock().expect("auth lock").push(auth);

    let body = if path == "/" {
        r#"{"name":"mock-node","cluster_name":"mock-cluster","cluster_uuid":"mock-uuid","version":{"number":"8.15.0","build_flavor":"default","build_type":"mock","build_hash":"mock","build_date":"2026-01-01T00:00:00.000Z","build_snapshot":false,"lucene_version":"9.0.0","minimum_wire_compatibility_version":"7.17.0","minimum_index_compatibility_version":"7.0.0"},"tagline":"You Know, for Search"}"#
    } else {
        "{}"
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn start_mock_elasticsearch(auth_headers: Arc<Mutex<Vec<String>>>) -> (u16, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let port = listener.local_addr().expect("local addr").port();
    listener
        .set_nonblocking(true)
        .expect("set mock server nonblocking");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_server = stop.clone();
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !stop_server.load(Ordering::Relaxed) && std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => handle_mock_request(stream, &auth_headers),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    (port, stop, handle)
}

fn remove_context(elastic: &Path, config: &Path) {
    let _ = Command::new(elastic)
        .args(["config", "context", "remove", "test", "--force", "--config-file"])
        .arg(config)
        .output();
}

#[test]
#[ignore = "requires the experimental elastic CLI, a working OS keychain, and interactive user approval for keychain access"]
fn elastic_cli_keyring_context_can_be_read_by_elasticrc_collect() {
    let elastic = elastic_path().expect("elastic CLI must be installed");
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let config = tmp.path().join(".elasticrc.yml");
    let output_dir = tmp.path().join("out");
    fs::create_dir(&output_dir).expect("create output dir");
    let auth_headers = Arc::new(Mutex::new(Vec::new()));
    let (port, stop_server, server) = start_mock_elasticsearch(auth_headers.clone());

    let add = Command::new(&elastic)
        .args([
            "config",
            "context",
            "add",
            "test",
            "--force",
            "--config-file",
        ])
        .arg(&config)
        .args([
            "--es-url",
            &format!("http://127.0.0.1:{port}"),
            "--es-api-key",
            "test-api-key",
        ])
        .output()
        .expect("add elastic context");
    assert!(
        add.status.success(),
        "failed to add context: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let collect = Command::new(env!("CARGO_BIN_EXE_esdiag"))
        .args(["collect", ".test.elasticsearch"])
        .arg(&output_dir)
        .args(["--type", "minimal"])
        .env("ELASTIC_CLI_CONFIG_FILE", &config)
        .output()
        .expect("run esdiag collect");

    remove_context(&elastic, &config);
    stop_server.store(true, Ordering::Relaxed);
    let _ = server.join();

    assert!(
        collect.status.success(),
        "collect failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&collect.stdout),
        String::from_utf8_lossy(&collect.stderr)
    );
    assert!(
        auth_headers
            .lock()
            .expect("auth lock")
            .iter()
            .any(|header| header == "ApiKey test-api-key"),
        "expected collection request to use keychain-backed API key"
    );
}
