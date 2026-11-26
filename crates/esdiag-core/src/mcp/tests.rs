// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::*;
use crate::{
    TestEnv,
    data::{Credential, KnownHost, SecretAuth, upsert_secret_auth},
    es_mock::ElasticsearchMock,
};
use rmcp::{
    ClientHandler, RoleClient, ServiceExt,
    model::{CallToolRequestParams, ClientRequest, Request},
    service::{NotificationContext, PeerRequestOptions, RunningService},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio::{io::AsyncReadExt, sync::mpsc::UnboundedReceiver};
use url::Url;

struct TestClient {
    progress: mpsc::UnboundedSender<ProgressNotificationParam>,
}

impl ClientHandler for TestClient {
    async fn on_progress(&self, params: ProgressNotificationParam, _context: NotificationContext<RoleClient>) {
        let _ = self.progress.send(params);
    }
}

fn local() -> McpPolicy {
    McpPolicy::Local(Arc::new(host::StdioHost))
}

async fn connect() -> (
    RunningService<RoleClient, TestClient>,
    UnboundedReceiver<ProgressNotificationParam>,
) {
    connect_with(local()).await
}

async fn connect_with(
    policy: McpPolicy,
) -> (
    RunningService<RoleClient, TestClient>,
    UnboundedReceiver<ProgressNotificationParam>,
) {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let server = EsdiagMcp::new(policy).serve(server_io).await.expect("serve");
        let _ = server.waiting().await;
    });
    let (progress, progress_rx) = mpsc::unbounded_channel();
    let client = TestClient { progress }.serve(client_io).await.expect("connect client");
    (client, progress_rx)
}

async fn call(client: &RunningService<RoleClient, TestClient>, name: &'static str, arguments: Value) -> CallToolResult {
    let Value::Object(arguments) = arguments else {
        panic!("tool arguments must be an object");
    };
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
        .await
        .expect("tools/call")
}

fn structured(result: &CallToolResult) -> &Value {
    result.structured_content.as_ref().expect("structured content")
}

fn fixture(name: &str) -> String {
    format!("{}/../../tests/archives/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn save_host(name: &str, url: &str, roles: Vec<HostRole>, credential: Credential) {
    KnownHost::new(
        Application::Elasticsearch,
        Url::parse(url).expect("valid url"),
        roles,
        None,
        false,
        credential,
    )
    .save(name)
    .expect("save host");
}

#[test]
fn failure_result_keeps_the_cli_failure() {
    let result = failure_result(CliFailure::new(
        CliFailureCategory::KeystoreFailed,
        "keystore is locked",
    ));

    assert_eq!(result.is_error, Some(true));
    let content = structured(&result);
    assert_eq!(content["result"], "command_failed");
    assert_eq!(content["category"], "keystore_failed");
    assert_eq!(content["message"], "keystore is locked");
}

#[test]
fn server_reports_esdiag_and_its_version() {
    let info = EsdiagMcp::new(local()).get_info();

    assert_eq!(info.server_info.name, "esdiag");
    assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn tools_follow_features_with_catalog_annotations() {
    // (read-only, destructive, idempotent, open-world)
    let mut expected = BTreeMap::from([
        ("host_list", (true, false, true, false)),
        ("host_auth", (true, false, true, true)),
        ("output_show", (true, false, true, false)),
        ("keystore_status", (true, false, true, false)),
        ("process", (false, false, false, true)),
        ("collect", (false, false, false, true)),
        ("send", (false, false, false, true)),
        ("host_add", (false, false, false, false)),
        ("host_update", (false, false, false, false)),
        ("output_set", (false, false, true, false)),
        ("host_remove", (false, true, false, false)),
        ("output_clear", (false, true, true, false)),
    ]);
    if cfg!(feature = "keystore") {
        expected.insert("job_list", (true, false, true, false));
        expected.insert("job_run", (false, false, false, true));
        expected.insert("job_delete", (false, true, false, false));
    }
    if cfg!(feature = "agent") {
        expected.insert("agent_ask", (false, false, false, true));
    }
    if cfg!(feature = "setup") {
        expected.insert("setup_output", (false, false, true, true));
        expected.insert("setup_elasticsearch", (false, false, true, true));
        expected.insert("setup_kibana", (false, false, true, true));
    }

    let listed = EsdiagMcp::new(local())
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| {
            assert!(tool.description.is_some(), "{} has no description", tool.name);
            let annotations = tool.annotations.expect("annotations");
            let hints = (
                annotations.read_only_hint.expect("read-only hint"),
                annotations.destructive_hint.expect("destructive hint"),
                annotations.idempotent_hint.expect("idempotent hint"),
                annotations.open_world_hint.expect("open-world hint"),
            );
            (tool.name.to_string(), hints)
        })
        .collect::<BTreeMap<_, _>>();
    let expected = expected
        .into_iter()
        .map(|(name, hints)| (name.to_string(), hints))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(listed, expected);
}

#[test]
fn schemas_accept_secret_references_but_no_credentials_or_sources() {
    let tools = EsdiagMcp::new(local()).tool_router.list_all();
    let properties = |name: &str| {
        let tool = tools.iter().find(|tool| tool.name == name).expect("tool listed");
        tool.input_schema["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };

    for name in ["host_add", "host_update"] {
        let properties = properties(name);
        assert!(properties.contains(&"secret".to_string()), "{name}: {properties:?}");
        for forbidden in ["apikey", "username", "password"] {
            assert!(!properties.contains(&forbidden.to_string()), "{name}: {properties:?}");
        }
    }
    for name in ["process", "collect"] {
        assert!(!properties(name).contains(&"sources".to_string()), "{name}");
    }
}

#[tokio::test]
async fn host_list_matches_the_cli_json_outcome() {
    let _env = TestEnv::new();
    save_host(
        "prod",
        "http://localhost:9200",
        vec![HostRole::Collect],
        Credential::None,
    );
    let (client, _) = connect().await;

    let result = call(&client, "host_list", json!({})).await;

    let CommandOutcome::Completed(outcome) = operations::host::host_list().expect("host list") else {
        panic!("host list streams nothing");
    };
    assert_ne!(result.is_error, Some(true));
    assert_eq!(structured(&result), &serde_json::to_value(outcome).expect("json"));
    let _ = client.cancel().await;
}

#[tokio::test]
async fn process_returns_the_diagnostic_and_reports_progress() {
    let env = TestEnv::new();
    let output = env.tmp.path().join("processed");
    std::fs::create_dir_all(&output).expect("output dir");
    let (client, mut progress) = connect().await;

    let result = call(
        &client,
        "process",
        json!({
            "input": fixture("logstash-api-diagnostics-9.3.3.zip"),
            "output": output.display().to_string(),
        }),
    )
    .await;

    assert_ne!(result.is_error, Some(true), "{:?}", result.structured_content);
    assert_eq!(structured(&result)["result"], "diagnostic_processed");
    let first = tokio::time::timeout(Duration::from_secs(5), progress.recv())
        .await
        .expect("progress notification")
        .expect("progress channel");
    let message = first.message.expect("progress message");
    assert!(
        ["collect", "load", "save", "process", "export", "send"]
            .iter()
            .any(|stage| message.starts_with(stage)),
        "{message}"
    );
    let _ = client.cancel().await;
}

#[tokio::test]
async fn process_rejects_stdout_output() {
    let _env = TestEnv::new();
    let (client, _) = connect().await;

    let result = call(
        &client,
        "process",
        json!({ "input": fixture("logstash-api-diagnostics-9.3.3.zip"), "output": "-" }),
    )
    .await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(&result)["category"], "invalid_input");
    let _ = client.cancel().await;
}

#[cfg(feature = "keystore")]
#[tokio::test]
async fn job_run_rejects_a_job_that_exports_to_stdout() {
    let _env = TestEnv::new();
    save_host("prod", "http://127.0.0.1:9", vec![HostRole::Collect], Credential::None);
    let job = crate::data::Job::try_new(
        Default::default(),
        crate::job::model::Input::Collect {
            host: "prod".to_string(),
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        },
        None,
        Some(crate::job::model::Process {
            selection: None,
            export: crate::job::model::ExportTarget::Stdout,
        }),
        None,
    )
    .expect("streaming job");
    operations::job::job_save("stream", job).expect("save job");
    let (client, mut progress) = connect().await;

    let result = call(&client, "job_run", json!({ "name": "stream" })).await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(&result)["category"], "invalid_input");
    assert!(progress.try_recv().is_err(), "no stage should start");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn schema_violations_are_rejected_before_running() {
    let _env = TestEnv::new();
    let (client, mut progress) = connect().await;

    for arguments in [
        json!({ "output": "/tmp" }),
        json!({ "host": "prod", "output": "/tmp", "sources": "/tmp/sources.yml" }),
    ] {
        let result = call(&client, "collect", arguments).await;

        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.is_none(), "the operation must not run");
        let text = serde_json::to_string(&result.content).expect("content");
        assert!(text.contains("failed to deserialize parameters"), "{text}");
    }
    assert!(progress.try_recv().is_err(), "no stage should start");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn concurrent_host_adds_both_persist() {
    let _env = TestEnv::new();
    let url = ElasticsearchMock::new().serve().await;
    let (client, _) = connect().await;

    let add = |name: &'static str| {
        call(
            &client,
            "host_add",
            json!({ "name": name, "target": url, "app": "elasticsearch", "roles": ["collect"] }),
        )
    };
    let (a, b) = tokio::join!(add("a"), add("b"));

    assert_ne!(a.is_error, Some(true), "{:?}", a.structured_content);
    assert_ne!(b.is_error, Some(true), "{:?}", b.structured_content);
    let hosts = structured(&call(&client, "host_list", json!({})).await).to_string();
    assert!(hosts.contains("\"a\"") && hosts.contains("\"b\""), "{hosts}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn locked_keystore_fails_collect_with_keystore_failed() {
    let env = TestEnv::new();
    upsert_secret_auth("prod-secret", SecretAuth::apikey("prod-key"), "pw").expect("save secret");
    save_host(
        "prod",
        "http://127.0.0.1:9",
        vec![HostRole::Collect],
        Credential::Keystore("prod-secret".to_string()),
    );
    let (client, _) = connect().await;

    let result = call(
        &client,
        "collect",
        json!({ "host": "prod", "output": env.tmp.path().display().to_string() }),
    )
    .await;

    assert_eq!(result.is_error, Some(true));
    let content = structured(&result);
    assert_eq!(content["category"], "keystore_failed");
    assert!(
        content["message"]
            .as_str()
            .expect("message")
            .contains("esdiag keystore unlock")
    );
    let _ = client.cancel().await;
}

#[tokio::test]
async fn urls_with_embedded_credentials_are_rejected_without_echoing_them() {
    let env = TestEnv::new();
    let output = env.tmp.path().display().to_string();
    let (client, mut progress) = connect().await;

    for (tool, arguments) in [
        (
            "host_add",
            json!({ "name": "plain", "target": "https://elastic:hunter2@example.test:9200", "app": "elasticsearch" }),
        ),
        (
            "host_add",
            json!({ "name": "tmpl", "target": "https://elastic:hunter2@{id}.example.test", "url_template": true }),
        ),
        (
            "process",
            json!({ "input": "https://token:hunter2@upload.elastic.co/d/abc123" }),
        ),
        (
            "collect",
            json!({ "host": "https://elastic:hunter2@example.test:9200", "output": output }),
        ),
    ] {
        let result = call(&client, tool, arguments).await;

        assert_eq!(result.is_error, Some(true), "{tool}");
        assert_eq!(structured(&result)["category"], "invalid_input", "{tool}");
        let text = serde_json::to_string(&result).expect("result");
        assert!(!text.contains("hunter2"), "{tool} echoed the credential: {text}");
    }
    assert!(progress.try_recv().is_err(), "no stage should start");
    let hosts = structured(&call(&client, "host_list", json!({})).await).to_string();
    assert!(!hosts.contains("plain") && !hosts.contains("tmpl"), "{hosts}");
    let _ = client.cancel().await;
}

#[cfg(feature = "keystore")]
#[tokio::test]
async fn locked_keystore_fails_a_saved_job_with_keystore_failed() {
    let env = TestEnv::new();
    upsert_secret_auth("prod-secret", SecretAuth::apikey("prod-key"), "pw").expect("save secret");
    save_host(
        "prod",
        "http://127.0.0.1:9",
        vec![HostRole::Collect],
        Credential::Keystore("prod-secret".to_string()),
    );
    let job = crate::data::Job::try_new(
        Default::default(),
        crate::job::model::Input::Collect {
            host: "prod".to_string(),
            diagnostic_type: "standard".to_string(),
            include: None,
            exclude: None,
        },
        Some(crate::job::model::SaveTarget::retained(env.tmp.path().to_path_buf())),
        None,
        None,
    )
    .expect("collect job");
    operations::job::job_save("nightly", job).expect("save job");
    let (client, _) = connect().await;

    let result = call(&client, "job_run", json!({ "name": "nightly" })).await;

    assert_eq!(result.is_error, Some(true));
    let content = structured(&result);
    assert_eq!(content["category"], "keystore_failed", "{content}");
    assert!(
        content["message"]
            .as_str()
            .expect("message")
            .contains("esdiag keystore unlock")
    );
    let _ = client.cancel().await;
}

#[cfg(feature = "setup")]
#[tokio::test]
async fn locked_keystore_fails_setup_with_keystore_failed() {
    let _env = TestEnv::new();
    upsert_secret_auth("prod-secret", SecretAuth::apikey("prod-key"), "pw").expect("save secret");
    save_host(
        "prod",
        "http://127.0.0.1:9",
        vec![HostRole::Send],
        Credential::Keystore("prod-secret".to_string()),
    );
    let (client, _) = connect().await;

    let result = call(&client, "setup_elasticsearch", json!({ "host": "prod" })).await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(&result)["category"], "keystore_failed", "{:?}", result);
    let _ = client.cancel().await;
}

#[tokio::test]
async fn cancelled_collect_stops_and_the_server_keeps_serving() {
    let env = TestEnv::new();
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = silent.local_addr().expect("address");
    save_host(
        "silent",
        &format!("http://{address}"),
        vec![HostRole::Collect],
        Credential::None,
    );
    let (client, _) = connect().await;

    let params = CallToolRequestParams::new("collect").with_arguments(
        json!({ "host": "silent", "output": env.tmp.path().display().to_string() })
            .as_object()
            .expect("object")
            .clone(),
    );
    let handle = client
        .send_cancellable_request(
            ClientRequest::CallToolRequest(Request::new(params)),
            PeerRequestOptions::no_options(),
        )
        .await
        .expect("send collect");
    let (mut connection, _) = tokio::time::timeout(Duration::from_secs(10), silent.accept())
        .await
        .expect("collect connects")
        .expect("accept");
    handle.cancel(None).await.expect("cancel");

    let mut buffer = [0_u8; 4096];
    let closed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match connection.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => continue,
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the cancelled collection kept its connection open");
    let result = call(&client, "host_list", json!({})).await;
    assert_ne!(result.is_error, Some(true));
    let _ = client.cancel().await;
}

#[derive(Default)]
struct FakeHost {
    identity: Option<RequestIdentity>,
    cap: usize,
    output: Option<std::path::PathBuf>,
    active: Arc<std::sync::Mutex<usize>>,
    owners: std::sync::Mutex<Vec<String>>,
    released: Arc<std::sync::Mutex<Vec<AdmissionRecord>>>,
}

impl FakeHost {
    fn new(identity: Option<RequestIdentity>) -> Self {
        Self {
            identity,
            cap: 1,
            ..Self::default()
        }
    }

    fn active(&self) -> usize {
        *self.active.lock().unwrap()
    }

    fn owners(&self) -> Vec<String> {
        self.owners.lock().unwrap().clone()
    }

    async fn wait_released(&self) -> AdmissionRecord {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(record) = self.released.lock().unwrap().last().copied() {
                    return record;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("slot released")
    }
}

impl McpHost for FakeHost {
    fn identity(&self, _headers: &http::HeaderMap) -> eyre::Result<Option<RequestIdentity>> {
        Ok(self.identity.clone())
    }

    fn admit(
        &self,
        owner: String,
    ) -> futures::future::BoxFuture<'_, Result<host::AdmissionGuard, crate::job::admission::JobAdmissionError>> {
        Box::pin(async move {
            let mut active = self.active.lock().unwrap();
            if *active >= self.cap {
                return Err(crate::job::admission::JobAdmissionError::OwnerCapReached {
                    owner,
                    active: *active,
                    cap: self.cap,
                });
            }
            *active += 1;
            self.owners.lock().unwrap().push(owner);
            let (slots, released) = (Arc::clone(&self.active), Arc::clone(&self.released));
            Ok(host::AdmissionGuard::new(move |record| async move {
                *slots.lock().unwrap() -= 1;
                released.lock().unwrap().push(record);
            }))
        })
    }

    fn exporter(&self) -> futures::future::BoxFuture<'_, eyre::Result<crate::exporter::Exporter>> {
        Box::pin(async move {
            let output = self.output.clone().ok_or_else(|| eyre::eyre!("no exporter"))?;
            crate::exporter::Exporter::try_from(Uri::Directory(output))
        })
    }
}

fn operator() -> Option<RequestIdentity> {
    Some(RequestIdentity {
        user: "operator@example.com".to_string(),
        account: Some("acme".to_string()),
    })
}

#[test]
fn the_request_identity_is_the_report_user() {
    let (user, account) = super::identified(&operator(), Some("mallory".into()), None);
    assert_eq!(user.as_deref(), Some("operator@example.com"));
    assert_eq!(account.as_deref(), Some("acme"));

    let (user, account) = super::identified(&operator(), Some("mallory".into()), Some("globex".into()));
    assert_eq!(user.as_deref(), Some("operator@example.com"));
    assert_eq!(account.as_deref(), Some("globex"), "a supplied account is kept");

    let (user, account) = super::identified(&None, Some("alice".into()), None);
    assert_eq!(user.as_deref(), Some("alice"), "stdio keeps the supplied user");
    assert_eq!(account, None);
}

#[tokio::test]
async fn a_completed_call_releases_its_slot_after_tracked_tasks_end() {
    let host = FakeHost::new(None);
    let mut guard = host.admit("a".to_string()).await.expect("admitted");
    let in_flight = Arc::new(tokio::sync::Notify::new());
    let task = Arc::clone(&in_flight);
    guard.scope().tracker().spawn(async move { task.notified().await });
    let record = AdmissionRecord::Finished {
        outcome: crate::processor::DiagnosticOutcome::Complete,
        docs: 3,
        errors: 0,
    };
    guard.finish(record);
    drop(guard);

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(host.active(), 1, "the slot is held while a tracked task runs");
    in_flight.notify_one();
    assert_eq!(host.wait_released().await, record);
    assert_eq!(host.active(), 0);
}

#[tokio::test]
async fn a_dropped_call_releases_its_slot_after_tracked_tasks_end() {
    let host = Arc::new(FakeHost::new(None));
    let in_flight = Arc::new(tokio::sync::Notify::new());
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let call = {
        let (host, in_flight) = (Arc::clone(&host), Arc::clone(&in_flight));
        tokio::spawn(async move {
            let guard = host.admit("a".to_string()).await.expect("admitted");
            let scope = guard.scope();
            with_scoped_execution(scope.clone(), async move {
                scope.tracker().spawn(async move { in_flight.notified().await });
                let _ = started_tx.send(());
                std::future::pending::<()>().await;
            })
            .await;
            drop(guard);
        })
    };
    started.await.expect("call started");
    call.abort();
    let _ = call.await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(host.active(), 1, "the slot is held while a tracked task runs");
    in_flight.notify_one();
    assert_eq!(host.wait_released().await, AdmissionRecord::Failed);
    assert_eq!(host.active(), 0);
}

#[test]
fn service_mode_lists_only_process_with_url_and_token() {
    let tools = EsdiagMcp::new(McpPolicy::Service(Arc::new(FakeHost::new(operator()))))
        .tool_router
        .list_all();

    assert_eq!(
        tools.iter().map(|tool| tool.name.to_string()).collect::<Vec<_>>(),
        ["process"]
    );
    let mut properties = tools[0].input_schema["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    properties.sort();
    assert_eq!(properties, ["token", "url"]);
}

#[test]
fn the_local_catalog_offers_no_token_parameter() {
    for tool in EsdiagMcp::new(local()).tool_router.list_all() {
        let properties = tool.input_schema["properties"].as_object().cloned().unwrap_or_default();
        assert!(!properties.contains_key("token"), "{}", tool.name);
    }
}

#[tokio::test]
async fn service_process_rejects_local_inputs_before_admission() {
    let env = TestEnv::new();
    save_host("prod", "http://127.0.0.1:9", vec![HostRole::Collect], Credential::None);
    let archive = fixture("logstash-api-diagnostics-9.3.3.zip");
    let token = "s3cr3t-download-token";
    let embedded = format!("https://token:{token}@upload.elastic.co/d/abc");
    let host = Arc::new(FakeHost::new(operator()));
    let (client, _) = connect_with(McpPolicy::Service(host.clone())).await;

    for url in [
        archive.as_str(),
        "prod",
        &env.tmp.path().display().to_string(),
        &embedded,
    ] {
        let result = call(&client, "process", json!({ "url": url, "token": token })).await;

        assert_eq!(result.is_error, Some(true), "{url}");
        assert_eq!(structured(&result)["category"], "invalid_input", "{url}");
        let text = serde_json::to_string(&result).expect("result json");
        assert!(!text.contains(token), "{url}: {text}");
    }
    assert!(host.owners().is_empty(), "no call was admitted");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn service_process_reports_the_cap_it_would_exceed() {
    let host = Arc::new(FakeHost {
        cap: 0,
        ..FakeHost::new(operator())
    });
    let (client, _) = connect_with(McpPolicy::Service(host)).await;

    let result = call(
        &client,
        "process",
        json!({ "url": "https://upload.elastic.co/d/abc", "token": "t" }),
    )
    .await;

    assert_eq!(result.is_error, Some(true));
    let message = structured(&result)["message"].as_str().expect("message").to_string();
    assert!(message.contains("per-owner concurrent job cap"), "{message}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn service_process_requires_a_request_identity() {
    let (client, _) = connect_with(McpPolicy::Service(Arc::new(FakeHost::new(None)))).await;

    let result = call(
        &client,
        "process",
        json!({ "url": "https://upload.elastic.co/d/abc", "token": "t" }),
    )
    .await;

    assert_eq!(result.is_error, Some(true));
    assert_eq!(structured(&result)["category"], "authentication_failed");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn local_process_records_the_request_identity() {
    let env = TestEnv::new();
    let output = env.tmp.path().join("processed");
    std::fs::create_dir_all(&output).expect("output dir");
    let host = Arc::new(FakeHost::new(operator()));
    let (client, _) = connect_with(McpPolicy::Local(host.clone())).await;

    let result = call(
        &client,
        "process",
        json!({
            "input": fixture("logstash-api-diagnostics-9.3.3.zip"),
            "output": output.display().to_string(),
        }),
    )
    .await;

    assert_ne!(result.is_error, Some(true), "{:?}", result.structured_content);
    let report = std::fs::read_to_string(output.join("metrics-diagnostic-esdiag.ndjson")).expect("report");
    assert!(report.contains("\"user\":\"operator@example.com\""), "{report}");
    assert!(report.contains("\"account\":\"acme\""), "{report}");
    assert_eq!(host.owners(), ["operator@example.com"]);
    assert!(matches!(host.wait_released().await, AdmissionRecord::Finished { .. }));
    let _ = client.cancel().await;
}

async fn serve_upload(token: &'static str, archive: Vec<u8>) -> Url {
    let app = axum::Router::new().fallback(move |headers: axum::http::HeaderMap| {
        let archive = archive.clone();
        async move {
            match headers.get("Authorization").and_then(|value| value.to_str().ok()) {
                Some(value) if value == token => (axum::http::StatusCode::OK, archive),
                _ => (axum::http::StatusCode::UNAUTHORIZED, Vec::new()),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = Url::parse(&format!("http://{}/", listener.local_addr().expect("addr"))).expect("url");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve upload") });
    url
}

#[tokio::test(flavor = "multi_thread")]
async fn service_process_exports_an_upload_with_the_request_identity() {
    let env = TestEnv::new();
    let output = env.tmp.path().join("processed");
    std::fs::create_dir_all(&output).expect("output dir");
    let token = "s3cr3t-download-token";
    let archive = std::fs::read(fixture("logstash-api-diagnostics-9.3.3.zip")).expect("archive");
    let upload = serve_upload(token, archive).await;
    upload_link::allow_test_origin(Some(&upload));
    let host = Arc::new(FakeHost {
        output: Some(output.clone()),
        ..FakeHost::new(operator())
    });
    let (client, _) = connect_with(McpPolicy::Service(host.clone())).await;

    let result = call(
        &client,
        "process",
        json!({ "url": upload.join("d/abc").expect("upload url").to_string(), "token": token }),
    )
    .await;
    upload_link::allow_test_origin(None);

    assert_ne!(result.is_error, Some(true), "{:?}", result.structured_content);
    assert_eq!(structured(&result)["result"], "diagnostic_processed");
    let text = serde_json::to_string(&result).expect("result json");
    assert!(!text.contains(token), "{text}");
    let report = std::fs::read_to_string(output.join("metrics-diagnostic-esdiag.ndjson")).expect("report");
    assert!(report.contains("\"user\":\"operator@example.com\""), "{report}");
    assert!(report.contains("\"account\":\"acme\""), "{report}");
    assert_eq!(host.owners(), ["operator@example.com"]);
    assert!(matches!(host.wait_released().await, AdmissionRecord::Finished { .. }));
    let _ = client.cancel().await;
}
