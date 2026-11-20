// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Elasticsearch mock shared by unit tests (`crate::es_mock`) and integration
//! tests (`tests/common`). It must not use `crate::` paths.
//!
//! The mock models the APIs ESDiag writes through the way a real cluster does:
//! - `POST /{index}/_bulk` requires an NDJSON body terminated by a newline and
//!   answers with one item per action. ESDiag targets are data streams, so any
//!   op other than `create` is rejected per item, as Elasticsearch does.
//! - `POST /{index}/_doc` answers with a write response, or a failure-store
//!   capture (`201` with `"failure_store":"used"`).
//! - `GET /` answers with the root info document.
//! - When an authorization is required, every route answers `401` without it.
//! - Unknown routes answer `400 no handler found`, so unexpected requests fail.

#![allow(dead_code, clippy::result_large_err)]

use axum::{
    Json, Router,
    body::Bytes,
    extract::{OriginalUri, Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Scripted answer for one `_bulk` request. Requests beyond the script get
/// [`BulkReply::Created`].
#[derive(Clone, Debug)]
pub enum BulkReply {
    /// Create every document, one realistic item per action.
    Created,
    /// Reject the whole request with an Elasticsearch error body.
    Status(u16),
    /// Explicit per-item results; one entry per action, each the inner item
    /// object (`status`, `_index`, `error`, `failure_store`, ...).
    Items(Vec<Value>),
}

/// Scripted answer for one `_doc` request. Requests beyond the script get
/// [`DocReply::Created`].
#[derive(Clone, Debug)]
pub enum DocReply {
    Created,
    /// The document failed ingest and was captured by the failure store.
    FailureStore,
    Status(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkAction {
    pub op: String,
    pub index: String,
    pub pipeline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BulkRequest {
    pub path_index: String,
    pub authorization: Option<String>,
    pub actions: Vec<BulkAction>,
    pub documents: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct DocRequest {
    pub index: String,
    pub pipeline: Option<String>,
    pub authorization: Option<String>,
    pub document: Value,
}

#[derive(Default)]
struct Recorded {
    bulk_replies: VecDeque<BulkReply>,
    doc_replies: VecDeque<DocReply>,
    bulk: Vec<BulkRequest>,
    docs: Vec<DocRequest>,
    created: usize,
    seq_no: u64,
    authorization: Option<String>,
}

#[derive(Clone)]
pub struct ElasticsearchMock {
    recorded: Arc<Mutex<Recorded>>,
    path_prefix: Arc<str>,
    cluster_name: Arc<str>,
    version: Arc<str>,
}

impl Default for ElasticsearchMock {
    fn default() -> Self {
        Self::new()
    }
}

impl ElasticsearchMock {
    pub fn new() -> Self {
        Self {
            recorded: Arc::default(),
            path_prefix: Arc::from(""),
            cluster_name: Arc::from("esdiag-mock"),
            version: Arc::from("9.3.3"),
        }
    }

    /// Require this exact `Authorization` header value on every request.
    pub fn require_authorization(self, value: &str) -> Self {
        self.set_required_authorization(value);
        self
    }

    /// Change the accepted credential of a running mock, as when a cluster
    /// rotates or revokes an API key.
    pub fn set_required_authorization(&self, value: &str) {
        self.lock().authorization = Some(value.to_string());
    }

    /// Serve the cluster below a path, as behind a proxy (e.g. the Elastic
    /// Cloud admin proxy `/api/v1/deployments/{id}/elasticsearch`).
    pub fn path_prefix(mut self, prefix: &str) -> Self {
        self.path_prefix = Arc::from(prefix.trim_end_matches('/'));
        self
    }

    pub fn cluster_name(mut self, name: &str) -> Self {
        self.cluster_name = Arc::from(name);
        self
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = Arc::from(version);
        self
    }

    pub fn bulk_replies(self, replies: impl IntoIterator<Item = BulkReply>) -> Self {
        self.lock().bulk_replies.extend(replies);
        self
    }

    pub fn doc_replies(self, replies: impl IntoIterator<Item = DocReply>) -> Self {
        self.lock().doc_replies.extend(replies);
        self
    }

    /// Routes for `GET /`, `POST /{index}/_bulk` and `POST /{index}/_doc`.
    /// Merge into a larger router to add other endpoints.
    pub fn router(&self) -> Router {
        self.routes().with_state(self.clone())
    }

    fn routes(&self) -> Router<ElasticsearchMock> {
        let prefix = self.path_prefix.as_ref();
        let routes = Router::new()
            .route(&format!("{prefix}/"), get(root))
            .route(&format!("{prefix}/{{index}}/_bulk"), post(bulk))
            .route(&format!("{prefix}/{{index}}/_doc"), post(doc));
        if prefix.is_empty() {
            routes
        } else {
            routes.route(prefix, get(root))
        }
    }

    /// Serve [`Self::router`] with an Elasticsearch-style fallback for unknown
    /// routes. Returns the base URL.
    pub async fn serve(&self) -> String {
        let app = self.routes().fallback(no_handler).with_state(self.clone());
        Self::serve_router(app).await
    }

    /// Serve an arbitrary router (typically one built from [`Self::router`]).
    pub async fn serve_router(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock Elasticsearch");
        let address = listener.local_addr().expect("mock Elasticsearch address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock Elasticsearch");
        });
        format!("http://{address}")
    }

    pub fn bulk_requests(&self) -> Vec<BulkRequest> {
        self.lock().bulk.clone()
    }

    pub fn doc_requests(&self) -> Vec<DocRequest> {
        self.lock().docs.clone()
    }

    /// Documents the mock answered with a `201` bulk item.
    pub fn created_documents(&self) -> usize {
        self.lock().created
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Recorded> {
        self.recorded.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn authorize(&self, headers: &HeaderMap, path: &str) -> Result<Option<String>, Response> {
        let provided = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let expected = self.lock().authorization.clone();
        match expected {
            Some(expected) if provided.as_deref() != Some(expected.as_str()) => {
                let reason = match provided {
                    None => format!("missing authentication credentials for REST request [{path}]"),
                    Some(_) => format!("unable to authenticate with provided credentials for REST request [{path}]"),
                };
                let mut response = error_response(StatusCode::UNAUTHORIZED, "security_exception", &reason);
                response.headers_mut().insert(
                    header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Basic realm=\"security\", charset=\"UTF-8\", ApiKey"),
                );
                Err(response)
            }
            _ => Ok(provided),
        }
    }
}

fn product_header(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert("x-elastic-product", HeaderValue::from_static("Elasticsearch"));
    response
}

fn error_response(status: StatusCode, error_type: &str, reason: &str) -> Response {
    let cause = json!({ "type": error_type, "reason": reason });
    let body = json!({
        "error": { "root_cause": [cause.clone()], "type": error_type, "reason": reason },
        "status": status.as_u16(),
    });
    product_header((status, Json(body)).into_response())
}

fn status_error(code: u16) -> Response {
    let status = StatusCode::from_u16(code).expect("valid HTTP status");
    let (error_type, reason) = match code {
        400 => ("illegal_argument_exception", "request rejected"),
        401 => ("security_exception", "unable to authenticate user"),
        403 => ("security_exception", "action is unauthorized for user"),
        404 => ("index_not_found_exception", "no such index"),
        413 => (
            "content_too_long_exception",
            "request content exceeds http.max_content_length",
        ),
        429 => (
            "es_rejected_execution_exception",
            "rejected execution of coordinating operation",
        ),
        503 => (
            "cluster_block_exception",
            "blocked by: [SERVICE_UNAVAILABLE/2/no master]",
        ),
        _ => ("exception", "internal server error"),
    };
    error_response(status, error_type, reason)
}

fn pipeline_param(query: Option<&str>) -> Option<String> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "pipeline")
        .map(|(_, value)| value.to_string())
}

fn accepts_content_type(headers: &HeaderMap, accepted: &[&str]) -> Result<(), Response> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if accepted.contains(&media_type) {
        return Ok(());
    }
    let body = json!({
        "error": format!("Content-Type header [{content_type}] is not supported"),
        "status": 406,
    });
    Err(product_header((StatusCode::NOT_ACCEPTABLE, Json(body)).into_response()))
}

/// Elasticsearch accepts gzip request bodies; ESDiag's client compresses them.
fn decoded_body(headers: &HeaderMap, body: Bytes) -> Result<Vec<u8>, Response> {
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("identity");
    match encoding {
        "identity" => Ok(body.to_vec()),
        "gzip" => {
            let mut decoded = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(body.as_ref()), &mut decoded)
                .map(|_| decoded)
                .map_err(|error| {
                    error_response(
                        StatusCode::BAD_REQUEST,
                        "illegal_argument_exception",
                        &error.to_string(),
                    )
                })
        }
        other => Err(error_response(
            StatusCode::BAD_REQUEST,
            "illegal_argument_exception",
            &format!("unsupported Content-Encoding [{other}]"),
        )),
    }
}

fn backing_index(prefix: &str, index: &str) -> String {
    format!("{prefix}-{index}-2026.01.01-000001")
}

async fn root(State(mock): State<ElasticsearchMock>, headers: HeaderMap) -> Response {
    if let Err(response) = mock.authorize(&headers, "/") {
        return response;
    }
    let cluster_name = mock.cluster_name.as_ref();
    product_header(
        Json(json!({
            "name": format!("{cluster_name}-node"),
            "cluster_name": cluster_name,
            "cluster_uuid": format!("{cluster_name}-uuid"),
            "version": {
                "number": mock.version.as_ref(),
                "build_flavor": "default",
                "build_type": "tar",
                "build_hash": "0000000000000000000000000000000000000000",
                "build_date": "2026-01-01T00:00:00.000Z",
                "build_snapshot": false,
                "lucene_version": "10.0.0",
                "minimum_wire_compatibility_version": "8.19.0",
                "minimum_index_compatibility_version": "8.0.0"
            },
            "tagline": "You Know, for Search"
        }))
        .into_response(),
    )
}

async fn no_handler(
    State(mock): State<ElasticsearchMock>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = mock.authorize(&headers, uri.path()) {
        return response;
    }
    let body = json!({
        "error": format!("no handler found for uri [{}] and method [{method}]", uri.path()),
        "status": 400,
    });
    product_header((StatusCode::BAD_REQUEST, Json(body)).into_response())
}

fn parse_bulk(path_index: &str, query: Option<&str>, body: &str) -> Result<(Vec<BulkAction>, Vec<Value>), Response> {
    let malformed = |reason: String| error_response(StatusCode::BAD_REQUEST, "illegal_argument_exception", &reason);
    if body.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "action_request_validation_exception",
            "Validation Failed: 1: no requests added;",
        ));
    }
    if !body.ends_with('\n') {
        return Err(malformed(
            "The bulk request must be terminated by a newline [\\n]".into(),
        ));
    }
    let default_pipeline = pipeline_param(query);
    let mut lines = body.lines().enumerate().filter(|(_, line)| !line.trim().is_empty());
    let mut actions = Vec::new();
    let mut documents = Vec::new();
    while let Some((number, line)) = lines.next() {
        let action: Map<String, Value> = serde_json::from_str(line).map_err(|_| {
            malformed(format!(
                "Malformed action/metadata line [{}], expected START_OBJECT",
                number + 1
            ))
        })?;
        let mut entries = action.into_iter();
        let (Some((op, metadata)), None) = (entries.next(), entries.next()) else {
            return Err(malformed(format!(
                "Malformed action/metadata line [{}], expected a single action",
                number + 1
            )));
        };
        if !matches!(op.as_str(), "create" | "index" | "update" | "delete") {
            return Err(malformed(format!(
                "Malformed action/metadata line [{}], expected field [create], [delete], [index] or [update] but found [{op}]",
                number + 1
            )));
        }
        let index = metadata
            .get("_index")
            .and_then(Value::as_str)
            .unwrap_or(path_index)
            .to_string();
        let pipeline = metadata
            .get("pipeline")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| default_pipeline.clone());
        if op != "delete" {
            let (_, source) = lines
                .next()
                .ok_or_else(|| malformed(format!("Missing source for action line [{}]", number + 1)))?;
            let document: Value = serde_json::from_str(source)
                .ok()
                .filter(Value::is_object)
                .ok_or_else(|| malformed(format!("Malformed source after action line [{}]", number + 1)))?;
            documents.push(document);
        }
        actions.push(BulkAction { op, index, pipeline });
    }
    Ok((actions, documents))
}

async fn bulk(
    State(mock): State<ElasticsearchMock>,
    Path(path_index): Path<String>,
    RawQuery(query): RawQuery,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let authorization = match mock.authorize(&headers, uri.path()) {
        Ok(authorization) => authorization,
        Err(response) => return response,
    };
    if let Err(response) = accepts_content_type(&headers, &["application/x-ndjson", "application/json"]) {
        return response;
    }
    let body = match decoded_body(&headers, body) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let Ok(body) = std::str::from_utf8(&body) else {
        return error_response(StatusCode::BAD_REQUEST, "illegal_argument_exception", "invalid UTF-8");
    };
    let (actions, documents) = match parse_bulk(&path_index, query.as_deref(), body) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };

    let mut recorded = mock.lock();
    recorded.bulk.push(BulkRequest {
        path_index,
        authorization,
        actions: actions.clone(),
        documents,
    });
    let reply = recorded.bulk_replies.pop_front().unwrap_or(BulkReply::Created);
    let items: Vec<Value> = match reply {
        BulkReply::Status(code) => return status_error(code),
        BulkReply::Items(items) if items.len() != actions.len() => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mock_misconfigured",
                &format!("scripted {} bulk items for {} actions", items.len(), actions.len()),
            );
        }
        BulkReply::Items(items) => actions
            .iter()
            .zip(items)
            .map(|(action, item)| json!({ action.op.clone(): item }))
            .collect(),
        BulkReply::Created => actions
            .iter()
            .map(|action| {
                if action.op != "create" {
                    return json!({ action.op.clone(): {
                        "_index": action.index,
                        "status": 400,
                        "error": {
                            "type": "illegal_argument_exception",
                            "reason": format!(
                                "only write ops with an op_type of create are allowed in data streams, index [{}]",
                                action.index
                            )
                        }
                    }});
                }
                recorded.seq_no += 1;
                json!({ "create": {
                    "_index": backing_index(".ds", &action.index),
                    "_id": format!("mock-{}", recorded.seq_no),
                    "_version": 1,
                    "result": "created",
                    "_shards": { "total": 2, "successful": 1, "failed": 0 },
                    "_seq_no": recorded.seq_no,
                    "_primary_term": 1,
                    "status": 201
                }})
            })
            .collect(),
    };
    let status_of = |item: &Value| {
        item.as_object()
            .and_then(|object| object.values().next())
            .and_then(|inner| inner["status"].as_u64())
            .unwrap_or(500)
    };
    recorded.created += items.iter().filter(|item| status_of(item) == 201).count();
    let errors = items.iter().any(|item| status_of(item) >= 300);
    product_header(Json(json!({ "took": 3, "errors": errors, "items": items })).into_response())
}

async fn doc(
    State(mock): State<ElasticsearchMock>,
    Path(index): Path<String>,
    RawQuery(query): RawQuery,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let authorization = match mock.authorize(&headers, uri.path()) {
        Ok(authorization) => authorization,
        Err(response) => return response,
    };
    if let Err(response) = accepts_content_type(&headers, &["application/json"]) {
        return response;
    }
    let body = match decoded_body(&headers, body) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let Some(document) = serde_json::from_slice::<Value>(&body).ok().filter(Value::is_object) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "document_parsing_exception",
            "failed to parse document",
        );
    };
    let mut recorded = mock.lock();
    recorded.docs.push(DocRequest {
        index: index.clone(),
        pipeline: pipeline_param(query.as_deref()),
        authorization,
        document,
    });
    let (prefix, failure_store) = match recorded.doc_replies.pop_front().unwrap_or(DocReply::Created) {
        DocReply::Status(code) => return status_error(code),
        DocReply::Created => (".ds", false),
        DocReply::FailureStore => (".fs", true),
    };
    recorded.seq_no += 1;
    let mut body = json!({
        "_index": backing_index(prefix, &index),
        "_id": format!("mock-{}", recorded.seq_no),
        "_version": 1,
        "result": "created",
        "_shards": { "total": 2, "successful": 1, "failed": 0 },
        "_seq_no": recorded.seq_no,
        "_primary_term": 1
    });
    if failure_store {
        body["failure_store"] = json!("used");
    }
    product_header((StatusCode::CREATED, Json(body)).into_response())
}
