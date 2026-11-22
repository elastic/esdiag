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

async fn connect() -> (
    RunningService<RoleClient, TestClient>,
    UnboundedReceiver<ProgressNotificationParam>,
) {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let server = EsdiagMcp::new(McpPolicy::Local).serve(server_io).await.expect("serve");
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
    let info = EsdiagMcp::new(McpPolicy::Local).get_info();

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

    let listed = EsdiagMcp::new(McpPolicy::Local)
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
    let tools = EsdiagMcp::new(McpPolicy::Local).tool_router.list_all();
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
