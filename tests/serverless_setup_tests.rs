// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

#![cfg(feature = "setup")]

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::IntoResponse,
};
use esdiag::client::{Client, ElasticsearchBuilder};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
struct MockCluster {
    client: Client,
    url: url::Url,
    paths: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

async fn run_kibana_setup(url: &url::Url, space: &str) -> std::process::Output {
    let home = tempfile::tempdir().unwrap();
    let hosts = home.path().join("hosts.yml");
    std::fs::write(
        &hosts,
        serde_json::to_vec(&serde_json::json!({
            "kibana": {"app":"kibana", "roles":["view"], "url":url.as_str()}
        }))
        .unwrap(),
    )
    .unwrap();
    let space = space.to_string();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .args(["setup", "kibana"])
            .env("HOME", home.path())
            .env("ESDIAG_HOME", home.path())
            .env("ESDIAG_HOSTS", hosts)
            .env("ESDIAG_KIBANA_SPACE", space)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn kibana_setup_refuses_a_second_space_before_importing() {
    let requests = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let recorded = requests.clone();
    let app = Router::new().fallback(move |request: Request<Body>| {
        let requests = recorded.clone();
        async move {
            let method = request.method().as_str().to_string();
            let path = request.uri().path().to_string();
            requests.lock().unwrap().push((method.clone(), path.clone()));
            let response = |status: StatusCode, value: Value| {
                (status, [("content-type", "application/json")], value.to_string()).into_response()
            };
            match (method.as_str(), path.as_str()) {
                ("GET", "/api/status") => response(
                    StatusCode::OK,
                    serde_json::json!({"version":{"number":"9.4.2","build_flavor":"serverless"}}),
                ),
                ("GET", "/api/spaces/space") => {
                    response(StatusCode::OK, serde_json::json!([{"id":"default"},{"id":"support"}]))
                }
                ("GET", "/s/support/api/saved_objects/_find") => response(
                    StatusCode::OK,
                    serde_json::json!({
                        "total": 1,
                        "saved_objects": [{"id":"regenerated","originId":"esdiag-readme"}]
                    }),
                ),
                ("GET", "/api/saved_objects/_find") => {
                    response(StatusCode::OK, serde_json::json!({"total": 0, "saved_objects": []}))
                }
                _ => response(StatusCode::INTERNAL_SERVER_ERROR, Value::Null),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: url::Url = format!("http://{}", listener.local_addr().unwrap()).parse().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let output = run_kibana_setup(&url, "_default").await;
    let rerun = run_kibana_setup(&url, "support").await;
    task.abort();

    let rerun_output = format!(
        "{}{}",
        String::from_utf8_lossy(&rerun.stdout),
        String::from_utf8_lossy(&rerun.stderr)
    );
    assert!(!rerun_output.contains("already installed"), "{rerun_output}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "setup must fail: {stdout}{stderr}");
    assert!(
        format!("{stdout}{stderr}").contains("already installed in the 'support' space"),
        "{stdout}{stderr}"
    );
    let requests = requests.lock().unwrap();
    let (first_run, rerun_requests) = requests.split_at(
        requests
            .iter()
            .rposition(|(_, path)| path == "/api/spaces/space")
            .unwrap(),
    );
    assert!(
        rerun_requests
            .iter()
            .any(|(_, path)| path == "/api/spaces/space/support"),
        "updating the existing space must continue to the Kibana sync: {rerun_requests:?}"
    );
    let requests = first_run;
    assert!(
        requests.iter().all(|(method, _)| method == "GET"),
        "setup must not write anything: {requests:?}"
    );
    assert!(
        !requests
            .iter()
            .any(|(_, path)| path.contains("_import") || path.contains("workflow")),
        "{requests:?}"
    );
}

#[tokio::test]
async fn deployment_metadata_handles_both_products_and_restricted_metadata() {
    for kibana in [false, true] {
        for (status, body, expected) in [
            (
                StatusCode::OK,
                r#"{"version":{"build_flavor":"serverless"}}"#,
                Some(true),
            ),
            (StatusCode::OK, r#"{"version":{"build_flavor":"default"}}"#, Some(false)),
            (StatusCode::OK, r#"{"version":{"number":"8.19.0"}}"#, Some(false)),
            (StatusCode::FORBIDDEN, "", Some(false)),
            (StatusCode::INTERNAL_SERVER_ERROR, "", None),
            (StatusCode::OK, "invalid", None),
        ] {
            let app = Router::new().fallback(move |request: Request<Body>| async move {
                let expected_path = if kibana { "/api/status" } else { "/" };
                assert!(
                    request.uri().path() == expected_path
                        || (!kibana
                            && request.uri().path() == "/_xpack/usage"
                            && matches!(
                                status,
                                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
                            ))
                );
                (
                    status,
                    [
                        ("content-type", "application/json"),
                        ("x-elastic-product", "Elasticsearch"),
                    ],
                    body,
                )
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap()).parse().unwrap();
            let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            let client = if kibana {
                Client::Kibana(esdiag::client::KibanaClient::try_new(url, esdiag::data::Auth::None).unwrap())
            } else {
                Client::Elasticsearch(ElasticsearchBuilder::new(url).build().unwrap())
            };
            let actual = client.is_serverless().await;
            task.abort();
            assert_eq!(actual.ok(), expected);
        }
    }
}

#[tokio::test]
async fn restricted_elasticsearch_metadata_uses_serverless_security_fallback() {
    let app = Router::new().fallback(|request: Request<Body>| async move {
        let (status, body) = match request.uri().path() {
            "/" => (StatusCode::FORBIDDEN, ""),
            "/_xpack/usage" => (StatusCode::GONE, ""),
            _ => (StatusCode::OK, "{}"),
        };
        (
            status,
            [
                ("content-type", "application/json"),
                ("x-elastic-product", "Elasticsearch"),
            ],
            body,
        )
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: url::Url = format!("http://{}", listener.local_addr().unwrap()).parse().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = Client::Elasticsearch(
        ElasticsearchBuilder::new(url.clone())
            .apikey("test-key".into())
            .build()
            .unwrap(),
    );
    assert!(client.is_serverless().await.unwrap());
    task.abort();
}

impl Drop for MockCluster {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn cluster(probe_status: StatusCode, probe_body: &'static str, asset_status: StatusCode) -> MockCluster {
    cluster_with_flavor(probe_status, probe_body, asset_status, probe_status == StatusCode::GONE).await
}

async fn cluster_with_flavor(
    probe_status: StatusCode,
    probe_body: &'static str,
    asset_status: StatusCode,
    serverless: bool,
) -> MockCluster {
    cluster_with_role_status(probe_status, probe_body, asset_status, serverless, asset_status).await
}

async fn cluster_with_role_status(
    probe_status: StatusCode,
    probe_body: &'static str,
    asset_status: StatusCode,
    serverless: bool,
    role_status: StatusCode,
) -> MockCluster {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let requests = paths.clone();
    let app = Router::new().fallback(move |request: Request<Body>| {
        let requests = requests.clone();
        async move {
            assert_eq!(request.headers()["authorization"], "ApiKey test-key");
            let path = request.uri().path().to_string();
            requests.lock().unwrap().push(path.clone());
            let (status, body) = if path == "/" {
                (
                    StatusCode::OK,
                    if serverless {
                        r#"{"version":{"build_flavor":"serverless"}}"#
                    } else {
                        r#"{"version":{"build_flavor":"default"}}"#
                    },
                )
            } else if path == "/_xpack/usage" {
                (probe_status, probe_body)
            } else if path.ends_with("/_mapping") {
                (StatusCode::OK, "{}")
            } else if path.starts_with("/_security/role/") && role_status != asset_status {
                (role_status, r#"{"error":"security_exception"}"#)
            } else {
                (asset_status, r#"{"acknowledged":true}"#)
            };
            (
                status,
                [
                    ("content-type", "application/json"),
                    ("x-elastic-product", "Elasticsearch"),
                ],
                body,
            )
                .into_response()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: url::Url = format!("http://{}", listener.local_addr().unwrap()).parse().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = Client::Elasticsearch(
        ElasticsearchBuilder::new(url.clone())
            .apikey("test-key".into())
            .build()
            .unwrap(),
    );
    MockCluster {
        client,
        url,
        paths,
        task,
    }
}

#[tokio::test]
async fn cli_setup_attempts_kibana_after_elasticsearch_asset_rejection() {
    let es = cluster(StatusCode::GONE, "", StatusCode::BAD_REQUEST).await;
    let paths = Arc::new(Mutex::new(Vec::new()));
    let requests = paths.clone();
    let app = Router::new().fallback(move |request: Request<Body>| {
        let requests = requests.clone();
        async move {
            requests.lock().unwrap().push(request.uri().path().to_string());
            if request.uri().path() == "/api/status" {
                (
                    StatusCode::OK,
                    r#"{"version":{"number":"9.4.2","build_flavor":"serverless"}}"#,
                )
            } else {
                // Stop at Kibana's first asset request. This test verifies that
                // Elasticsearch's rejection cannot prevent that phase.
                (StatusCode::SERVICE_UNAVAILABLE, "{}")
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let kb_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let tmp = tempfile::tempdir().unwrap();
    let output_url = es.url.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_esdiag"))
            .arg("setup")
            .env("ESDIAG_HOME", tmp.path())
            .env("ESDIAG_OUTPUT_URL", output_url)
            .env("ESDIAG_KIBANA_URL", kb_url)
            .env("ESDIAG_KIBANA_SPACE", "esdiag")
            .env("ESDIAG_OUTPUT_APIKEY", "test-key")
            .env_remove("ESDIAG_OUTPUT_USERNAME")
            .env_remove("ESDIAG_OUTPUT_PASSWORD")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    task.abort();
    assert!(!output.status.success(), "Kibana's own failure must still fail setup");
    assert!(
        paths.lock().unwrap().iter().any(|path| path.contains("/spaces/")),
        "Kibana asset phase was skipped: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn security_probe_preserves_stateful_behavior_and_handles_gone() {
    for (status, body, expected) in [
        (StatusCode::OK, r#"{"security":{"enabled":true}}"#, true),
        (StatusCode::OK, r#"{"security":{"enabled":false}}"#, false),
        (StatusCode::OK, "{}", true),
        (StatusCode::UNAUTHORIZED, "", true),
        (StatusCode::FORBIDDEN, "", true),
        (StatusCode::NOT_FOUND, "", false),
        (StatusCode::GONE, "not JSON", true),
    ] {
        let mock = cluster(status, body, StatusCode::OK).await;
        assert_eq!(mock.client.has_security_enabled().await.unwrap(), expected);
    }
    for (status, body) in [
        (StatusCode::INTERNAL_SERVER_ERROR, ""),
        (StatusCode::OK, "invalid JSON"),
    ] {
        let mock = cluster(status, body, StatusCode::OK).await;
        assert!(mock.client.has_security_enabled().await.is_err());
    }
}

#[tokio::test]
async fn setup_installs_assets_and_skips_roles_only_when_needed() {
    for (status, body, expects_roles) in [
        (StatusCode::GONE, "", true),
        (StatusCode::OK, r#"{"security":{"enabled":false}}"#, false),
        (StatusCode::OK, r#"{"security":{"enabled":true}}"#, true),
        (StatusCode::FORBIDDEN, "", true),
    ] {
        let mock = cluster(status, body, StatusCode::OK).await;
        esdiag::setup::assets(&mock.client).await.unwrap();
        let paths = mock.paths.lock().unwrap();
        assert_eq!(
            paths.iter().filter(|p| *p == "/_xpack/usage").count(),
            usize::from(status != StatusCode::GONE)
        );
        for prefix in ["/_ingest/pipeline/", "/_component_template/", "/_index_template/"] {
            assert!(paths.iter().any(|p| p.starts_with(prefix)), "missing {prefix}");
        }
        assert_eq!(paths.iter().any(|p| p.starts_with("/_security/role/")), expects_roles);
    }
}

#[tokio::test]
async fn setup_still_reports_probe_and_asset_failures() {
    let mock = cluster(StatusCode::INTERNAL_SERVER_ERROR, "", StatusCode::OK).await;
    assert!(esdiag::setup::assets(&mock.client).await.is_err());
    assert_eq!(*mock.paths.lock().unwrap(), vec!["/", "/_xpack/usage"]);

    let mock = cluster(StatusCode::GONE, "", StatusCode::FORBIDDEN).await;
    assert!(esdiag::setup::assets(&mock.client).await.is_err());
    let report = esdiag::setup::assets_report(&mock.client)
        .await
        .expect("asset failures must remain reportable so Kibana setup can proceed");
    assert!(!report.is_complete());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("metrics-diagnostic"))
    );
}

#[tokio::test]
async fn serverless_metadata_avoids_stateful_security_and_license_apis() {
    let mock = cluster_with_flavor(StatusCode::INTERNAL_SERVER_ERROR, "", StatusCode::OK, true).await;
    esdiag::setup::assets(&mock.client).await.unwrap();
    esdiag::setup::ensure_agent_builder_license(&mock.client).await.unwrap();
    let paths = mock.paths.lock().unwrap();
    assert!(!paths.iter().any(|p| {
        (p.starts_with("/_security/") && !p.starts_with("/_security/role/"))
            || p == "/_xpack/usage"
            || p.starts_with("/_license")
    }));
    assert!(paths.iter().any(|p| p.starts_with("/_security/role/esdiag-user")));
    assert!(paths.iter().any(|p| p.starts_with("/_index_template/")));
}

#[tokio::test]
async fn forbidden_role_install_warns_only_on_serverless() {
    let mock = cluster_with_role_status(StatusCode::GONE, "", StatusCode::OK, true, StatusCode::FORBIDDEN).await;
    let report = esdiag::setup::assets_report(&mock.client).await.unwrap();
    assert!(report.is_complete(), "unexpected warnings: {:?}", report.warnings);
    assert!(
        mock.paths
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.starts_with("/_security/role/esdiag-user"))
    );
    esdiag::setup::assets(&mock.client).await.unwrap();

    let mock = cluster_with_role_status(
        StatusCode::OK,
        r#"{"security":{"enabled":true}}"#,
        StatusCode::OK,
        false,
        StatusCode::FORBIDDEN,
    )
    .await;
    let report = esdiag::setup::assets_report(&mock.client).await.unwrap();
    assert!(
        report.warnings.iter().any(|warning| warning.contains("esdiag-user")),
        "stateful role rejection must stay visible: {:?}",
        report.warnings
    );
}

#[derive(Default)]
struct TemplateState {
    templates: HashMap<String, Value>,
    rollovers: Vec<String>,
}

async fn template_cluster(state: Arc<Mutex<TemplateState>>) -> (Client, tokio::task::JoinHandle<()>) {
    let app = Router::new().fallback(move |request: Request<Body>| {
        let state = state.clone();
        async move {
            let method = request.method().clone();
            let path = request.uri().path().to_string();
            let body = to_bytes(request.into_body(), usize::MAX).await.unwrap();
            let mut state = state.lock().unwrap();
            let template_kind = ["_index_template", "_component_template"]
                .into_iter()
                .find(|kind| path.starts_with(&format!("/{kind}/")));
            let (status, body) = if path == "/" {
                (StatusCode::OK, r#"{"version":{"build_flavor":"default"}}"#.to_string())
            } else if path == "/_xpack/usage" {
                (StatusCode::OK, r#"{"security":{"enabled":false}}"#.to_string())
            } else if let Some(kind) = template_kind {
                let name = path.rsplit('/').next().unwrap().to_string();
                if method == "PUT" {
                    let body = flate2::read::GzDecoder::new(&body[..]);
                    state.templates.insert(path, serde_json::from_reader(body).unwrap());
                    (StatusCode::OK, r#"{"acknowledged":true}"#.to_string())
                } else if let Some(template) = state.templates.get(&path) {
                    let (list, item) = match kind {
                        "_index_template" => ("index_templates", "index_template"),
                        _ => ("component_templates", "component_template"),
                    };
                    let body = serde_json::json!({ list: [{ "name": name, item: template }] });
                    (StatusCode::OK, body.to_string())
                } else {
                    (StatusCode::NOT_FOUND, "{}".to_string())
                }
            } else if path.starts_with("/_data_stream/") {
                let body = serde_json::json!({ "data_streams": [
                    { "name": "metrics-logstash-esdiag", "template": "metrics-logstash-esdiag" },
                    { "name": "settings-cluster-esdiag", "template": "settings-cluster-esdiag" },
                    { "name": "custom-esdiag", "template": "custom" },
                ]});
                (StatusCode::OK, body.to_string())
            } else if let Some(stream) = path.strip_suffix("/_rollover") {
                state.rollovers.push(stream.trim_start_matches('/').to_string());
                (StatusCode::OK, r#"{"acknowledged":true}"#.to_string())
            } else if path.ends_with("/_mapping") {
                (StatusCode::OK, "{}".to_string())
            } else {
                (StatusCode::OK, r#"{"acknowledged":true}"#.to_string())
            };
            (
                status,
                [
                    ("content-type", "application/json"),
                    ("x-elastic-product", "Elasticsearch"),
                ],
                body,
            )
                .into_response()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url: url::Url = format!("http://{}", listener.local_addr().unwrap()).parse().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = Client::Elasticsearch(
        ElasticsearchBuilder::new(url)
            .apikey("test-key".into())
            .build()
            .unwrap(),
    );
    (client, task)
}

#[tokio::test]
async fn setup_rolls_over_only_data_streams_with_updated_templates() {
    let state = Arc::new(Mutex::new(TemplateState::default()));
    let (client, task) = template_cluster(state.clone()).await;
    let take_rollovers = || std::mem::take(&mut state.lock().unwrap().rollovers);

    esdiag::setup::assets(&client).await.unwrap();
    assert_eq!(
        take_rollovers(),
        vec!["metrics-logstash-esdiag", "settings-cluster-esdiag"],
        "templates that were not installed before count as updated"
    );

    esdiag::setup::assets(&client).await.unwrap();
    assert!(
        take_rollovers().is_empty(),
        "templates at the bundled version must not roll over"
    );

    state
        .lock()
        .unwrap()
        .templates
        .get_mut("/_index_template/settings-cluster-esdiag")
        .unwrap()["_meta"]["description"] = Value::from("edited");
    esdiag::setup::assets(&client).await.unwrap();
    assert!(
        take_rollovers().is_empty(),
        "content changes without a version bump must not roll over"
    );

    {
        let mut state = state.lock().unwrap();
        let component = state
            .templates
            .get_mut("/_component_template/esdiag@ls-metadata")
            .unwrap();
        component["version"] = Value::from(component["version"].as_u64().unwrap() - 1);
    }
    esdiag::setup::assets(&client).await.unwrap();
    assert_eq!(
        take_rollovers(),
        vec!["metrics-logstash-esdiag"],
        "a component template version bump rolls over streams whose templates compose it"
    );

    let newer = {
        let mut state = state.lock().unwrap();
        let template = state
            .templates
            .get_mut("/_index_template/settings-cluster-esdiag")
            .unwrap();
        let newer = template["version"].as_u64().unwrap() + 1;
        template["version"] = Value::from(newer);
        newer
    };
    esdiag::setup::assets(&client).await.unwrap();
    assert_eq!(
        state.lock().unwrap().templates["/_index_template/settings-cluster-esdiag"]["version"],
        newer,
        "an older bundled template must not overwrite a newer installed one"
    );
    assert!(take_rollovers().is_empty());
    task.abort();
}
