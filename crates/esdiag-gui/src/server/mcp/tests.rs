// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{AllowedHosts, GuiMcpHost};
use crate::{
    data::{KnownHostBuilder, Uri},
    exporter::Exporter,
    server::{AuthProvider, JobConcurrencyCaps, RuntimeMode, Server, ServerStartOptions},
};
use axum::http::header;
use esdiag_core::{
    es_mock::{BulkReply, ElasticsearchMock},
    mcp::{
        EsdiagMcp, McpPolicy,
        host::{McpHost, StdioHost},
        upload_link,
    },
};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::Notify, task::JoinHandle};
use url::Url;

const TOKEN: &str = "s3cr3t-download-token";
const OPERATOR: &str = "operator@example.com";

#[test]
fn allowed_hosts_prefer_the_cli_then_the_environment() {
    let mut env = crate::TestEnv::new();
    env.set("ESDIAG_MCP_ALLOWED_HOSTS", "env.example.com");

    let cli = AllowedHosts::resolve(Some("cli.example.com")).expect("cli hosts");
    assert!(cli.allows_host("cli.example.com"));
    assert!(!cli.allows_host("env.example.com"));
    let from_env = AllowedHosts::resolve(None).expect("env hosts");
    assert!(from_env.allows_host("env.example.com:443"));
    assert!(!from_env.allows_host("localhost"));

    env.remove("ESDIAG_MCP_ALLOWED_HOSTS");
    let default = AllowedHosts::resolve(None).expect("default hosts");
    for host in ["localhost:2501", "127.0.0.1", "[::1]:2501"] {
        assert!(default.allows_host(host), "{host}");
    }
    assert!(!default.allows_host("attacker.example"));
}

#[test]
fn allowed_hosts_reject_empty_and_malformed_lists() {
    for value in [
        "",
        " , ",
        "https://example.com",
        "user@example.com",
        "example.com:notaport",
    ] {
        assert!(AllowedHosts::resolve(Some(value)).is_err(), "{value}");
    }
}

#[test]
fn origins_derive_from_allowed_hosts() {
    let hosts = AllowedHosts::resolve(Some("esdiag.example.com:8443, ::1")).expect("hosts");

    assert_eq!(
        hosts.origins(),
        [
            "http://esdiag.example.com:8443",
            "https://esdiag.example.com:8443",
            "http://[::1]:*",
            "https://[::1]:*",
        ]
    );
    assert_eq!(hosts.authorities(), ["esdiag.example.com:8443", "::1"]);
    assert!(hosts.allows_origin("https://esdiag.example.com:8443"));
    assert!(!hosts.allows_origin("https://esdiag.example.com"));
    assert!(hosts.allows_origin("http://[::1]:2501"));
    assert!(!hosts.allows_origin("https://evil.example"));
    assert!(!hosts.allows_origin("null"));
}

struct Endpoint {
    server: Server,
    port: u16,
    client: reqwest::Client,
}

impl Endpoint {
    async fn start(mode: RuntimeMode, exporter: Exporter, options: ServerStartOptions<'_>) -> Self {
        let (server, addr) = Server::start_with_options([127, 0, 0, 1], 0, exporter, String::new(), mode, options)
            .await
            .expect("server starts");
        Self {
            server,
            port: addr.port(),
            client: reqwest::Client::new(),
        }
    }

    async fn user(web_features: &str) -> Self {
        Self::start(
            RuntimeMode::User,
            Exporter::default(),
            ServerStartOptions {
                web_features: Some(web_features),
                mcp_endpoint: true,
                ..ServerStartOptions::default()
            },
        )
        .await
    }

    /// Service mode without a proxy, identified by `ESDIAG_USER`, one job per owner.
    async fn service(exporter: Exporter) -> Self {
        Self::start(
            RuntimeMode::Service,
            exporter,
            ServerStartOptions {
                auth_provider: Some(AuthProvider::None),
                job_caps: Some(JobConcurrencyCaps {
                    global: 8,
                    per_owner: 1,
                }),
                web_features: Some("mcp"),
                mcp_endpoint: true,
                ..ServerStartOptions::default()
            },
        )
        .await
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    /// Waits for the configured owner's job slot to be released.
    async fn wait_for_free_slot(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.server.state.active_jobs_for(OPERATOR).await > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the owner's slot is released");
    }

    fn post(&self, body: Value) -> reqwest::RequestBuilder {
        self.client
            .post(self.url())
            .header(header::ACCEPT, "application/json, text/event-stream")
            .json(&body)
    }

    /// Every JSON-RPC message in the response, from a JSON body or an event stream.
    async fn messages(&self, body: Value) -> (String, Vec<Value>) {
        let response = self.post(body).send().await.expect("mcp request");
        assert!(response.status().is_success(), "status {}", response.status());
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let text = response.text().await.expect("mcp body");
        (content_type, parse_messages(&text))
    }

    async fn call(&self, name: &str, arguments: Value) -> Value {
        let (_, messages) = self.messages(tool_call(name, arguments)).await;
        messages
            .into_iter()
            .find_map(|message| message.get("result").cloned())
            .expect("tool result")
    }
}

fn parse_messages(text: &str) -> Vec<Value> {
    if let Ok(message) = serde_json::from_str::<Value>(text) {
        return vec![message];
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str(data.trim()).ok())
        .collect()
}

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" }
        }
    })
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/archives")
        .join(name)
}

fn message(result: &Value) -> String {
    result["structuredContent"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn identified_env() -> crate::TestEnv {
    let mut env = crate::TestEnv::new();
    env.set("ESDIAG_USER", OPERATOR);
    env.set("ESDIAG_ACCOUNT", "acme");
    env
}

/// Serves `archive` to requests carrying the download token.
async fn serve_upload(archive: Vec<u8>) -> Url {
    let app = axum::Router::new().fallback(move |headers: axum::http::HeaderMap| {
        let archive = archive.clone();
        async move {
            match headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) {
                Some(TOKEN) => (axum::http::StatusCode::OK, archive),
                _ => (axum::http::StatusCode::UNAUTHORIZED, Vec::new()),
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind upload mock");
    let url = Url::parse(&format!("http://{}/d/upload", listener.local_addr().expect("addr"))).expect("url");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("upload mock") });
    upload_link::allow_test_origin(Some(&url));
    url
}

/// An upload service that accepts connections and never answers.
async fn stalled_upload() -> (TcpListener, Url) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stalled mock");
    let url = Url::parse(&format!("http://{}/d/upload", listener.local_addr().expect("addr"))).expect("url");
    upload_link::allow_test_origin(Some(&url));
    (listener, url)
}

async fn accept_request(listener: &TcpListener) -> tokio::net::TcpStream {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("download connects")
        .expect("accept");
    let mut request = [0u8; 4096];
    assert!(socket.read(&mut request).await.expect("read request") > 0);
    socket
}

async fn assert_closed(mut socket: tokio::net::TcpStream) {
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), socket.read_to_end(&mut rest))
        .await
        .expect("the download connection is closed")
        .ok();
}

fn spawn_process(endpoint: &Endpoint, url: &Url) -> JoinHandle<Option<Value>> {
    let request = endpoint.post(tool_call("process", json!({ "url": url.to_string(), "token": TOKEN })));
    tokio::spawn(async move {
        let text = request.send().await.ok()?.text().await.ok()?;
        parse_messages(&text)
            .into_iter()
            .find_map(|message| message.get("result").cloned())
    })
}

async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition reached");
}

/// Polls until a call from the same owner is admitted, returning its result.
async fn admitted_call(endpoint: &Endpoint, url: &Url) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let result = endpoint
                .call("process", json!({ "url": url.to_string(), "token": TOKEN }))
                .await;
            if !message(&result).contains("cap reached") {
                return result;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the owner's slot is released")
}

#[tokio::test]
async fn initialize_succeeds_when_mcp_is_enabled() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::user("advanced,mcp").await;

    let (_, messages) = endpoint.messages(initialize()).await;

    let result = &messages[0]["result"];
    assert_eq!(result["serverInfo"]["name"], "esdiag");
    assert!(result["capabilities"]["tools"].is_object(), "{result}");
}

#[tokio::test]
async fn mcp_is_not_found_without_the_web_feature() {
    let _env = crate::TestEnv::new();
    for mode in [RuntimeMode::User, RuntimeMode::Service] {
        let endpoint = Endpoint::start(
            mode,
            Exporter::default(),
            ServerStartOptions {
                auth_provider: Some(AuthProvider::None),
                web_features: Some(""),
                mcp_endpoint: true,
                ..ServerStartOptions::default()
            },
        )
        .await;

        let response = endpoint.post(initialize()).send().await.expect("request");

        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND, "{mode}");
    }
}

#[tokio::test]
async fn requests_served_without_a_session() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::user("mcp").await;

    let (_, messages) = endpoint
        .messages(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .await;

    assert!(
        messages[0]["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
    );
}

#[tokio::test]
async fn disallowed_host_and_origin_are_forbidden() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::user("mcp").await;

    let host = endpoint
        .post(initialize())
        .header(header::HOST, "attacker.example")
        .send()
        .await
        .expect("request");
    let origin = endpoint
        .post(initialize())
        .header(header::HOST, format!("127.0.0.1:{}", endpoint.port))
        .header(header::ORIGIN, "https://evil.example")
        .send()
        .await
        .expect("request");

    assert_eq!(host.status(), reqwest::StatusCode::FORBIDDEN);
    assert_eq!(origin.status(), reqwest::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_configured_public_hostname_is_allowed() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::start(
        RuntimeMode::User,
        Exporter::default(),
        ServerStartOptions {
            web_features: Some("mcp"),
            mcp_endpoint: true,
            mcp_allowed_hosts: Some("esdiag.example.com"),
            ..ServerStartOptions::default()
        },
    )
    .await;

    let response = endpoint
        .post(initialize())
        .header(header::HOST, "esdiag.example.com")
        .send()
        .await
        .expect("request");

    assert!(response.status().is_success(), "{}", response.status());
}

#[tokio::test]
async fn the_identity_aware_proxy_header_is_required() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::start(
        RuntimeMode::Service,
        Exporter::default(),
        ServerStartOptions {
            auth_provider: Some(AuthProvider::GoogleIap),
            web_features: Some("mcp"),
            mcp_endpoint: true,
            ..ServerStartOptions::default()
        },
    )
    .await;

    let mcp = endpoint.post(initialize()).send().await.expect("request");
    let index = endpoint
        .client
        .get(format!("http://127.0.0.1:{}/", endpoint.port))
        .send()
        .await
        .expect("request");

    assert!(mcp.status().is_client_error(), "{}", mcp.status());
    assert_eq!(mcp.status(), index.status());

    let rebound = endpoint
        .post(initialize())
        .header(header::HOST, "attacker.example")
        .send()
        .await
        .expect("request");
    assert_eq!(rebound.status(), reqwest::StatusCode::FORBIDDEN);
    let body = rebound.text().await.expect("body");
    assert!(
        body.contains("Host header is not allowed"),
        "the host check runs first: {body}"
    );
}

#[tokio::test]
async fn user_mode_lists_the_same_tools_as_stdio() {
    let _env = crate::TestEnv::new();
    let endpoint = Endpoint::user("mcp").await;
    let (_, messages) = endpoint
        .messages(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .await;
    let http = messages[0]["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().expect("name").to_string(),
                tool["inputSchema"].clone(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let server = EsdiagMcp::new(McpPolicy::Local(Arc::new(StdioHost)))
            .serve(server_io)
            .await
            .expect("serve");
        let _ = server.waiting().await;
    });
    let client = ().serve(client_io).await.expect("connect client");
    let stdio = client
        .list_all_tools()
        .await
        .expect("list tools")
        .into_iter()
        .map(|tool| {
            let schema = serde_json::to_value(tool.input_schema.as_ref()).expect("schema");
            (tool.name.to_string(), schema)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let _ = client.cancel().await;

    assert_eq!(http, stdio);
}

#[tokio::test]
async fn concurrent_host_adds_both_persist() {
    let env = crate::TestEnv::new();
    let url = ElasticsearchMock::new().serve().await;
    let endpoint = Endpoint::user("mcp").await;

    let add = |name: &'static str| {
        endpoint.call(
            "host_add",
            json!({ "name": name, "target": url, "app": "elasticsearch", "roles": ["collect"] }),
        )
    };
    let (a, b) = tokio::join!(add("a"), add("b"));

    assert_ne!(a["isError"], true, "{a}");
    assert_ne!(b["isError"], true, "{b}");
    let hosts = std::fs::read_to_string(&env.hosts_path).expect("hosts.yml");
    assert!(hosts.contains("a:") && hosts.contains("b:"), "{hosts}");
}

#[tokio::test]
async fn process_streams_progress_before_the_result() {
    let env = crate::TestEnv::new();
    let output = env.tmp.path().join("processed");
    std::fs::create_dir_all(&output).expect("output dir");
    let endpoint = Endpoint::user("mcp").await;
    let mut request = tool_call(
        "process",
        json!({
            "input": fixture("logstash-api-diagnostics-9.3.3.zip").display().to_string(),
            "output": output.display().to_string(),
        }),
    );
    request["params"]["_meta"] = json!({ "progressToken": "p1" });

    let (content_type, messages) = endpoint.messages(request).await;

    assert!(content_type.starts_with("text/event-stream"), "{content_type}");
    let progress = messages
        .iter()
        .position(|message| message["method"] == "notifications/progress")
        .expect("progress notification");
    let result = messages
        .iter()
        .position(|message| message.get("result").is_some())
        .expect("result");
    assert!(progress < result);
    assert_eq!(
        messages[result]["result"]["structuredContent"]["result"],
        "diagnostic_processed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn service_process_records_the_configured_identity() {
    let env = identified_env();
    let output = env.tmp.path().join("processed");
    std::fs::create_dir_all(&output).expect("output dir");
    let archive = std::fs::read(fixture("logstash-api-diagnostics-9.3.3.zip")).expect("archive");
    let upload = serve_upload(archive).await;
    let endpoint = Endpoint::service(Exporter::try_from(Uri::Directory(output.clone())).expect("exporter")).await;

    let result = endpoint
        .call("process", json!({ "url": upload.to_string(), "token": TOKEN }))
        .await;
    upload_link::allow_test_origin(None);

    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["result"], "diagnostic_processed");
    assert!(!result.to_string().contains(TOKEN));
    let report = std::fs::read_to_string(output.join("metrics-diagnostic-esdiag.ndjson")).expect("report");
    assert!(report.contains(&format!("\"user\":\"{OPERATOR}\"")), "{report}");
    assert!(report.contains("\"account\":\"acme\""), "{report}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_call_releases_its_slot() {
    let _env = identified_env();
    let upload = serve_upload(b"not a zip".to_vec()).await;
    let endpoint = Endpoint::service(Exporter::default()).await;

    for _ in 0..2 {
        let result = endpoint
            .call("process", json!({ "url": upload.to_string(), "token": TOKEN }))
            .await;

        assert_eq!(result["isError"], true, "{result}");
        assert!(!message(&result).contains("cap reached"), "{result}");
    }
    upload_link::allow_test_origin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_during_a_download_closes_it_and_frees_the_slot() {
    let _env = identified_env();
    let (listener, upload) = stalled_upload().await;
    let endpoint = Endpoint::service(Exporter::default()).await;

    let call = spawn_process(&endpoint, &upload);
    let socket = accept_request(&listener).await;
    call.abort();
    assert!(call.await.is_err(), "the call reports no result");
    assert_closed(socket).await;

    let next = spawn_process(&endpoint, &upload);
    drop(accept_request(&listener).await);
    let result = next.await.expect("next call").expect("next result");
    assert!(
        !message(&result).contains("cap reached"),
        "the next call is admitted: {result}"
    );
    upload_link::allow_test_origin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_during_a_download_closes_it() {
    let _env = identified_env();
    let (listener, upload) = stalled_upload().await;
    let mut endpoint = Endpoint::service(Exporter::default()).await;

    let call = spawn_process(&endpoint, &upload);
    let socket = accept_request(&listener).await;
    endpoint.server.shutdown().await;

    assert_closed(socket).await;
    let result = tokio::time::timeout(Duration::from_secs(10), call)
        .await
        .expect("the call ends")
        .expect("join");
    assert!(
        result.is_none_or(|result| result["structuredContent"]["result"] != "diagnostic_processed"),
        "the call is not reported as successful"
    );
    endpoint.wait_for_free_slot().await;
    upload_link::allow_test_origin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_dropped_during_admission_releases_its_slot() {
    let _env = identified_env();
    let endpoint = Endpoint::service(Exporter::default()).await;
    let state = Arc::clone(&endpoint.server.state);
    let stats = state.stats.write().await;

    let host = GuiMcpHost(Arc::clone(&state));
    let admission = tokio::time::timeout(Duration::from_millis(100), host.admit(OPERATOR.to_string())).await;
    assert!(admission.is_err(), "admission waits on the stats lock");
    assert_eq!(state.active_jobs_for(OPERATOR).await, 1, "the slot was taken");

    drop(stats);
    endpoint.wait_for_free_slot().await;
}

struct StalledExport {
    _env: crate::TestEnv,
    endpoint: Endpoint,
    mock: ElasticsearchMock,
    release: Arc<Notify>,
    upload: Url,
}

/// A service endpoint exporting to a mock whose second bulk request stalls.
async fn stalled_export() -> StalledExport {
    let mut env = identified_env();
    env.set("ESDIAG_ES_BULK_SIZE", "5");
    let release = Arc::new(Notify::new());
    let mock = ElasticsearchMock::new().bulk_replies([BulkReply::Created, BulkReply::Held(Arc::clone(&release))]);
    let url = Url::parse(&mock.serve().await).expect("mock url");
    let exporter = Exporter::try_from(KnownHostBuilder::new(url).build().expect("host")).expect("exporter");
    let archive = std::fs::read(fixture("logstash-api-diagnostics-9.3.3.zip")).expect("archive");
    let upload = serve_upload(archive).await;
    let endpoint = Endpoint::service(exporter).await;
    StalledExport {
        _env: env,
        endpoint,
        mock,
        release,
        upload,
    }
}

impl StalledExport {
    /// Bulk requests stop at the count seen once the call is stopped.
    async fn assert_no_further_bulk_requests(&self) -> usize {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let stopped = self.mock.bulk_requests().len();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            self.mock.bulk_requests().len(),
            stopped,
            "no bulk request after cancellation"
        );
        stopped
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnect_during_an_export_holds_the_slot_until_the_request_ends() {
    let stalled = stalled_export().await;
    let call = spawn_process(&stalled.endpoint, &stalled.upload);
    wait_for(|| stalled.mock.bulk_requests().len() >= 2).await;

    call.abort();
    assert!(call.await.is_err(), "the call reports no result");
    let stopped = stalled.assert_no_further_bulk_requests().await;
    let rejected = stalled
        .endpoint
        .call("process", json!({ "url": stalled.upload.to_string(), "token": TOKEN }))
        .await;
    assert!(
        message(&rejected).contains(&format!("per-owner concurrent job cap reached for {OPERATOR}")),
        "the slot is held while the request is in flight: {rejected}"
    );
    assert_eq!(stalled.mock.bulk_requests().len(), stopped);

    stalled.release.notify_one();
    let admitted = admitted_call(&stalled.endpoint, &stalled.upload).await;
    assert_eq!(
        admitted["structuredContent"]["result"], "diagnostic_processed",
        "{admitted}"
    );
    upload_link::allow_test_origin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_during_an_export_sends_no_further_bulk_request() {
    let mut stalled = stalled_export().await;
    let call = spawn_process(&stalled.endpoint, &stalled.upload);
    wait_for(|| stalled.mock.bulk_requests().len() >= 2).await;

    stalled.endpoint.server.shutdown().await;
    let result = tokio::time::timeout(Duration::from_secs(10), call)
        .await
        .expect("the call ends")
        .expect("join");
    assert!(
        result.is_none_or(|result| result["structuredContent"]["result"] != "diagnostic_processed"),
        "the call is not reported as successful"
    );
    let stopped = stalled.assert_no_further_bulk_requests().await;
    assert_eq!(
        stalled.endpoint.server.state.active_jobs_for(OPERATOR).await,
        1,
        "the slot is held while the request is in flight"
    );

    stalled.release.notify_one();
    stalled.endpoint.wait_for_free_slot().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        stalled.mock.bulk_requests().len(),
        stopped,
        "the released request starts no further sub-batch"
    );
    upload_link::allow_test_origin(None);
}
