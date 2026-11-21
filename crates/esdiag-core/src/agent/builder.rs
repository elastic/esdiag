// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Agent Builder's private SSE transport.
//!
//! This module owns the Kibana request shape and evolving event protocol. CLI
//! callers receive progress callbacks plus one finite completion or safe
//! failure, rather than a public Agent Builder event stream.

use crate::client::KibanaClient;
use futures::StreamExt;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, mem};
use url::Url;

const CONVERSE_PATH: &str = "api/agent_builder/converse/async";
const AGENTS_PATH: &str = "api/agent_builder/agents";

/// A configured Kibana viewer and its normalized Agent Builder space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentBuilderLocation {
    viewer: Url,
    space: Option<String>,
}

impl AgentBuilderLocation {
    /// Creates an Agent Builder location from the configured Kibana viewer.
    ///
    /// A trailing `/s/<space>` segment in the viewer URL is normalized away so
    /// request paths and handoff links are space-scoped exactly once.
    pub fn new(mut viewer: Url, space: Option<String>) -> Self {
        viewer.set_query(None);
        viewer.set_fragment(None);

        let path_segments: Vec<_> = viewer
            .path_segments()
            .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
            .unwrap_or_default();
        let url_space = path_segments
            .windows(2)
            .last()
            .filter(|segments| segments[0] == "s")
            .map(|segments| segments[1].to_owned());
        if url_space.is_some() {
            let prefix = &path_segments[..path_segments.len().saturating_sub(2)];
            viewer.set_path(&format!("/{}", prefix.join("/")));
        }

        Self {
            viewer,
            space: space
                .or(url_space)
                .filter(|s| !s.is_empty() && s != "_default" && s != "default"),
        }
    }

    /// Returns the normalized Kibana viewer URL without a space segment.
    pub fn viewer(&self) -> &Url {
        &self.viewer
    }

    /// Returns the configured space, or `None` for Kibana's default space.
    pub fn space(&self) -> Option<&str> {
        self.space.as_deref()
    }

    fn api_path(&self, endpoint: &str) -> String {
        let base = self.viewer.path().trim_end_matches('/');
        let endpoint = endpoint.trim_start_matches('/');
        match self.space.as_deref() {
            Some(space) if !space.is_empty() => {
                format!("{base}/s/{}/{}", urlencoding::encode(space), endpoint)
            }
            _ => format!("{base}/{endpoint}"),
        }
    }

    /// Constructs the Kibana handoff URL for a persisted conversation.
    pub fn conversation_url(&self, conversation_id: &str) -> Url {
        let mut url = self.viewer.clone();
        url.set_path(&self.api_path(&format!(
            "app/agent_builder/conversations/{}",
            urlencoding::encode(conversation_id)
        )));
        url
    }

    fn resolve_link(&self, path: &str) -> String {
        let mut url = self.viewer.clone();
        let viewer_path = self.viewer.path().trim_end_matches('/');
        let (path, fragment) = path.trim_start_matches('/').split_once('#').unwrap_or((path, ""));
        let path = if viewer_path.is_empty() || viewer_path == "/" {
            format!("/{path}")
        } else {
            format!("{viewer_path}/{path}")
        };
        url.set_path(&path);
        if !fragment.is_empty() {
            url.set_fragment(Some(fragment));
        }
        url.to_string()
    }
}

/// Inputs for one finite Agent Builder conversation turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    /// Agent Builder agent identifier.
    pub agent_id: String,
    /// Opaque question for the configured agent.
    pub prompt: String,
    /// Existing Kibana conversation to continue, when explicitly supplied.
    pub conversation_id: Option<String>,
}

/// Progress that a CLI can render to stderr while waiting for completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentProgress {
    Reasoning(String),
    ToolCall { tool_id: String },
    ToolProgress(String),
    ToolResult { tool_id: String },
}

/// Safe, optional inference usage reported by Agent Builder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// The one finite response produced by a completed Agent Builder turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCompletion {
    pub conversation_id: String,
    pub message: String,
    pub kibana_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentUsage>,
}

/// Categorized failures that contain no request credential or raw response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFailure {
    Transport,
    Http {
        status: u16,
    },
    Protocol {
        event: String,
    },
    Remote,
    Interrupted {
        conversation_id: Option<String>,
        kibana_url: Option<String>,
    },
}

impl AgentFailure {
    /// Whether callers may safely retry without risking a duplicate turn.
    pub fn retry_safe(&self) -> bool {
        !matches!(
            self,
            Self::Interrupted {
                conversation_id: Some(_),
                ..
            }
        )
    }
}

impl std::fmt::Display for AgentFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport => write!(f, "Unable to reach the configured Kibana Agent Builder endpoint"),
            Self::Http { status } => write!(f, "Agent Builder request failed with HTTP {status}"),
            Self::Protocol { event } => write!(f, "Agent Builder emitted an invalid {event} event"),
            Self::Remote => write!(f, "Agent Builder reported an error"),
            Self::Interrupted {
                conversation_id: Some(id),
                ..
            } => write!(f, "Agent Builder stream ended before completion for conversation {id}"),
            Self::Interrupted {
                conversation_id: None, ..
            } => write!(f, "Agent Builder stream ended before completion"),
        }
    }
}

impl std::error::Error for AgentFailure {}

/// A focused Agent Builder adapter over an already-configured [`KibanaClient`].
pub struct AgentBuilderClient<'a> {
    client: &'a KibanaClient,
    location: AgentBuilderLocation,
}

impl<'a> AgentBuilderClient<'a> {
    pub fn new(client: &'a KibanaClient, location: AgentBuilderLocation) -> Self {
        Self { client, location }
    }

    /// Retrieves the selected agent's user-facing name without exposing its
    /// broader definition to callers.
    pub async fn agent_name(&self, agent_id: &str) -> Result<String, AgentFailure> {
        let response = self
            .client
            .request(
                Method::GET,
                &HashMap::new(),
                &self
                    .location
                    .api_path(&format!("{AGENTS_PATH}/{}", urlencoding::encode(agent_id))),
                None,
            )
            .await
            .map_err(|_| AgentFailure::Transport)?;
        if !response.status().is_success() {
            return Err(AgentFailure::Http {
                status: response.status().as_u16(),
            });
        }
        let body = response.bytes().await.map_err(|_| AgentFailure::Transport)?;
        let agent: Value = serde_json::from_slice(&body).map_err(|_| AgentFailure::Protocol {
            event: "agent definition".to_owned(),
        })?;
        string_field(&agent, "name")
            .map(str::to_owned)
            .ok_or_else(|| AgentFailure::Protocol {
                event: "agent definition".to_owned(),
            })
    }

    /// Sends one turn, surfaces operational progress, then returns a finite
    /// completion. Agent Builder's raw SSE protocol never leaves this module.
    pub async fn ask<F>(&self, request: AgentRequest, mut progress: F) -> Result<AgentCompletion, AgentFailure>
    where
        F: FnMut(AgentProgress),
    {
        let mut body = json!({
            "agent_id": request.agent_id,
            "input": request.prompt,
        });
        if let Some(conversation_id) = request.conversation_id.as_deref() {
            body["conversation_id"] = Value::String(conversation_id.to_owned());
        }
        let body = serde_json::to_vec(&body).expect("Agent Builder request is serializable");
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), "application/json".to_owned());

        let response = self
            .client
            .request(
                Method::POST,
                &headers,
                &self.location.api_path(CONVERSE_PATH),
                Some(&body),
            )
            .await
            .map_err(|_| AgentFailure::Transport)?;
        if !response.status().is_success() {
            return Err(AgentFailure::Http {
                status: response.status().as_u16(),
            });
        }

        let mut decoder = SseDecoder::default();
        let mut state = StreamState {
            conversation_id: request.conversation_id,
            ..StreamState::default()
        };
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| self.interrupted(&state))?;
            for event in decoder.push(&chunk) {
                self.consume(event, &mut state, &mut progress)?;
            }
        }
        for event in decoder.finish() {
            self.consume(event, &mut state, &mut progress)?;
        }

        match (&state.message, &state.conversation_id) {
            (Some(message), Some(conversation_id)) => Ok(AgentCompletion {
                kibana_url: self.location.conversation_url(conversation_id).to_string(),
                conversation_id: conversation_id.clone(),
                message: resolve_relative_links(&self.location, message),
                usage: state.usage.clone(),
            }),
            _ => Err(self.interrupted(&state)),
        }
    }

    fn consume<F>(&self, event: SseEvent, state: &mut StreamState, progress: &mut F) -> Result<(), AgentFailure>
    where
        F: FnMut(AgentProgress),
    {
        if !matches!(
            event.name.as_str(),
            "conversation_id_set"
                | "reasoning"
                | "tool_call"
                | "tool_progress"
                | "tool_result"
                | "message_complete"
                | "round_complete"
                | "error"
        ) {
            return Ok(());
        }
        let data: Value = serde_json::from_str(&event.data).map_err(|_| AgentFailure::Protocol {
            event: event.name.clone(),
        })?;
        let payload = data.get("data").unwrap_or(&data);
        match event.name.as_str() {
            "conversation_id_set" => {
                state.conversation_id = string_field(payload, "conversation_id")
                    .map(str::to_owned)
                    .or(state.conversation_id.take());
            }
            "reasoning" => {
                if let Some(reasoning) = string_field(payload, "reasoning") {
                    progress(AgentProgress::Reasoning(reasoning.to_owned()));
                }
            }
            "tool_call" => {
                if let Some(tool_id) = string_field(payload, "tool_id") {
                    progress(AgentProgress::ToolCall {
                        tool_id: tool_id.to_owned(),
                    });
                }
            }
            "tool_progress" => {
                if let Some(message) = string_field(payload, "message") {
                    progress(AgentProgress::ToolProgress(message.to_owned()));
                }
            }
            "tool_result" => {
                if let Some(tool_id) = string_field(payload, "tool_id") {
                    progress(AgentProgress::ToolResult {
                        tool_id: tool_id.to_owned(),
                    });
                }
            }
            "message_complete" => {
                state.message = string_field(payload, "message_content")
                    .map(str::to_owned)
                    .or(state.message.take());
            }
            "round_complete" => {
                state.usage = usage_from(payload.get("round").and_then(|round| round.get("model_usage")));
            }
            "error" => return Err(AgentFailure::Remote),
            _ => unreachable!("unknown events return before payload parsing"),
        }
        Ok(())
    }

    fn interrupted(&self, state: &StreamState) -> AgentFailure {
        let kibana_url = state
            .conversation_id
            .as_deref()
            .map(|id| self.location.conversation_url(id).to_string());
        AgentFailure::Interrupted {
            conversation_id: state.conversation_id.clone(),
            kibana_url,
        }
    }
}

fn string_field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn usage_from(value: Option<&Value>) -> Option<AgentUsage> {
    let value = value?;
    let usage = AgentUsage {
        input_tokens: value.get("input_tokens").and_then(Value::as_u64),
        output_tokens: value.get("output_tokens").and_then(Value::as_u64),
    };
    (usage.input_tokens.is_some() || usage.output_tokens.is_some()).then_some(usage)
}

fn resolve_relative_links(location: &AgentBuilderLocation, message: &str) -> String {
    let mut resolved = String::with_capacity(message.len());
    let mut remainder = message;
    while let Some(start) = remainder.find("](") {
        let Some(end) = remainder[start..].find(')') else {
            break;
        };
        let end = start + end;
        let target = &remainder[start + 2..end];
        let path = target
            .strip_prefix('<')
            .and_then(|target| target.strip_suffix('>'))
            .unwrap_or(target);
        resolved.push_str(&remainder[..start + 2]);
        if path.starts_with('/') {
            resolved.push_str(&location.resolve_link(path));
        } else {
            resolved.push_str(target);
        }
        resolved.push(')');
        remainder = &remainder[end + 1..];
    }
    resolved.push_str(remainder);
    resolved
}

#[derive(Default)]
struct StreamState {
    conversation_id: Option<String>,
    message: Option<String>,
    usage: Option<AgentUsage>,
}

#[derive(Debug, PartialEq, Eq)]
struct SseEvent {
    name: String,
    data: String,
}

#[derive(Default)]
struct SseDecoder {
    pending: Vec<u8>,
    name: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.pending.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            if let Some(event) = self.line(&line) {
                events.push(event);
            }
        }
        events
    }

    fn finish(&mut self) -> Vec<SseEvent> {
        let mut line = mem::take(&mut self.pending);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        let line = String::from_utf8_lossy(&line);
        let mut events = self.line(&line).into_iter().collect::<Vec<_>>();
        if let Some(event) = self.dispatch() {
            events.push(event);
        }
        events
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        if let Some(value) = line.strip_prefix("event:") {
            self.name = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data.push(value.trim_start().to_owned());
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let data = mem::take(&mut self.data);
        (!data.is_empty()).then(|| SseEvent {
            name: self.name.take().unwrap_or_else(|| "message".to_owned()),
            data: data.join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Auth;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::Mutex,
    };

    #[test]
    fn location_scopes_once_and_builds_handoff() {
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example/base/s/esdiag").expect("url"),
            Some("esdiag".to_owned()),
        );

        assert_eq!(location.viewer().as_str(), "https://kb.example/base");
        assert_eq!(
            location.api_path(CONVERSE_PATH),
            "/base/s/esdiag/api/agent_builder/converse/async"
        );
        assert_eq!(
            location.conversation_url("conv 1").as_str(),
            "https://kb.example/base/s/esdiag/app/agent_builder/conversations/conv%201"
        );
    }

    #[test]
    fn location_uses_kibana_default_space_when_requested() {
        let location = AgentBuilderLocation::new(Url::parse("https://kb.example").expect("url"), None);

        assert_eq!(location.api_path(CONVERSE_PATH), "/api/agent_builder/converse/async");
    }

    #[test]
    fn explicit_default_overrides_viewer_space() {
        for space in ["_default", "", "default"] {
            let location = AgentBuilderLocation::new(
                Url::parse("https://kb.example/proxy/s/ops").unwrap(),
                Some(space.to_string()),
            );
            assert_eq!(location.space(), None);
            assert_eq!(
                location.api_path("api/agent_builder/agents"),
                "/proxy/api/agent_builder/agents"
            );
        }
    }

    #[test]
    fn decoder_handles_fragmented_events_and_keepalives() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"event: reasoning\ndata: {\"data\":").is_empty());
        let events = decoder.push(b"{\"reasoning\":\"thinking\"}}\n\n: ping\n\n");

        assert_eq!(
            events,
            vec![SseEvent {
                name: "reasoning".to_owned(),
                data: "{\"data\":{\"reasoning\":\"thinking\"}}".to_owned(),
            }]
        );
    }

    #[test]
    fn decoder_preserves_utf8_split_between_transport_chunks() {
        let mut decoder = SseDecoder::default();
        assert!(
            decoder
                .push(b"event: reasoning\ndata: {\"data\":{\"reasoning\":\"caf\xc3")
                .is_empty()
        );
        let events = decoder.push(b"\xa9\"}}\n\n");

        assert_eq!(events[0].data, "{\"data\":{\"reasoning\":\"caf\u{e9}\"}}");
    }

    #[test]
    fn decoder_keeps_multiline_data_together() {
        let mut decoder = SseDecoder::default();
        let events = decoder.push(b"event: message\ndata: one\ndata: two\n\n");

        assert_eq!(events[0].data, "one\ntwo");
    }

    #[test]
    fn relative_kibana_links_use_the_configured_viewer() {
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("url"),
            Some("esdiag".to_owned()),
        );
        let message = "[Dashboard](</s/esdiag/app/dashboards#/view/report>)";

        assert_eq!(
            resolve_relative_links(&location, message),
            "[Dashboard](https://kb.example/s/esdiag/app/dashboards#/view/report)"
        );
    }

    #[test]
    fn interruption_after_conversation_is_not_retry_safe() {
        let failure = AgentFailure::Interrupted {
            conversation_id: Some("conv-1".to_owned()),
            kibana_url: Some("https://kb.example/app/agent_builder/conversations/conv-1".to_owned()),
        };

        assert!(!failure.retry_safe());
    }

    #[tokio::test]
    async fn client_posts_a_finite_turn_and_consumes_a_completed_recorded_stream() {
        let stream = concat!(
            "event: conversation_id_set\n",
            "data: {\"data\":{\"conversation_id\":\"conv-123\"}}\n\n",
            "event: reasoning\n",
            "data: {\"data\":{\"reasoning\":\"Checking diagnostic\"}}\n\n",
            "event: tool_call\n",
            "data: {\"data\":{\"tool_id\":\"platform.core.execute_esql\"}}\n\n",
            "event: message_complete\n",
            "data: {\"data\":{\"message_content\":\"[Dashboard](</s/esdiag/app/dashboards#/view/report>)\"}}\n\n",
            "event: round_complete\n",
            "data: {\"data\":{\"round\":{\"model_usage\":{\"input_tokens\":12,\"output_tokens\":3}}}}\n\n"
        );
        let (url, request) = serve_sse(stream).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );
        let progress = Arc::new(StdMutex::new(Vec::new()));
        let collected_progress = progress.clone();

        let completion = AgentBuilderClient::new(&client, location)
            .ask(
                AgentRequest {
                    agent_id: "elastic-ai-agent".to_owned(),
                    prompt: "Analyze diagnostic abc123".to_owned(),
                    conversation_id: None,
                },
                move |event| {
                    collected_progress.lock().expect("progress lock").push(event);
                },
            )
            .await
            .expect("completion");

        assert_eq!(completion.conversation_id, "conv-123");
        assert_eq!(completion.usage.expect("usage").input_tokens, Some(12));
        assert_eq!(
            completion.message,
            "[Dashboard](https://kb.example/s/esdiag/app/dashboards#/view/report)"
        );
        assert_eq!(
            progress.lock().expect("progress lock").as_slice(),
            [
                AgentProgress::Reasoning("Checking diagnostic".to_owned()),
                AgentProgress::ToolCall {
                    tool_id: "platform.core.execute_esql".to_owned()
                }
            ]
        );

        let request = request.lock().await;
        assert!(request.starts_with("POST /s/esdiag/api/agent_builder/converse/async HTTP/1.1"));
        assert!(request.contains("\"agent_id\":\"elastic-ai-agent\""));
        assert!(request.contains("\"input\":\"Analyze diagnostic abc123\""));
        assert!(!request.contains("conversation_id"));
    }

    #[tokio::test]
    async fn client_reads_the_selected_agent_display_name() {
        let (url, request) = serve_json(r#"{"id":"diagnostic-agent","name":"Diagnostic Agent"}"#).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );

        let name = AgentBuilderClient::new(&client, location)
            .agent_name("diagnostic-agent")
            .await
            .expect("agent name");

        assert_eq!(name, "Diagnostic Agent");
        let request = request.lock().await;
        assert!(request.starts_with("GET /s/esdiag/api/agent_builder/agents/diagnostic-agent HTTP/1.1"));
    }

    #[tokio::test]
    async fn unknown_non_json_events_do_not_block_completion() {
        let stream = concat!(
            "event: conversation_id_set\n",
            "data: {\"data\":{\"conversation_id\":\"conv-123\"}}\n\n",
            "event: future_progress\n",
            "data: this event is not JSON\n\n",
            "event: message_complete\n",
            "data: {\"data\":{\"message_content\":\"Completed despite a future event\"}}\n\n"
        );
        let (url, _) = serve_sse(stream).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );

        let completion = AgentBuilderClient::new(&client, location)
            .ask(
                AgentRequest {
                    agent_id: "elastic-ai-agent".to_owned(),
                    prompt: "Analyze diagnostic abc123".to_owned(),
                    conversation_id: None,
                },
                |_| {},
            )
            .await
            .expect("completion");

        assert_eq!(completion.conversation_id, "conv-123");
        assert_eq!(completion.message, "Completed despite a future event");
    }

    #[tokio::test]
    async fn continuation_uses_the_supplied_conversation_when_sse_omits_it() {
        let stream = concat!(
            "event: reasoning\n",
            "data: {\"data\":{\"reasoning\":\"Continuing the prior analysis\"}}\n\n",
            "event: message_complete\n",
            "data: {\"data\":{\"message_content\":\"Follow-up answer\"}}\n\n",
            "event: round_complete\n",
            "data: {\"data\":{\"round\":{\"model_usage\":{\"input_tokens\":3,\"output_tokens\":2}}}}\n\n"
        );
        let (url, request) = serve_sse(stream).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );

        let completion = AgentBuilderClient::new(&client, location)
            .ask(
                AgentRequest {
                    agent_id: "elastic-ai-agent".to_owned(),
                    prompt: "Explain further".to_owned(),
                    conversation_id: Some("conv-123".to_owned()),
                },
                |_| {},
            )
            .await
            .expect("completion");

        assert_eq!(completion.conversation_id, "conv-123");
        assert_eq!(
            completion.kibana_url,
            "https://kb.example/s/esdiag/app/agent_builder/conversations/conv-123"
        );
        assert_eq!(completion.message, "Follow-up answer");

        let request = request.lock().await;
        assert!(request.contains("\"conversation_id\":\"conv-123\""));
    }

    #[tokio::test]
    async fn interrupted_continuation_returns_the_supplied_recovery_location() {
        let stream = concat!(
            "event: reasoning\n",
            "data: {\"data\":{\"reasoning\":\"Continuing the prior analysis\"}}\n\n"
        );
        let (url, _) = serve_sse(stream).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );

        let failure = AgentBuilderClient::new(&client, location)
            .ask(
                AgentRequest {
                    agent_id: "elastic-ai-agent".to_owned(),
                    prompt: "Explain further".to_owned(),
                    conversation_id: Some("conv-123".to_owned()),
                },
                |_| {},
            )
            .await
            .expect_err("interrupted stream");

        assert_eq!(
            failure,
            AgentFailure::Interrupted {
                conversation_id: Some("conv-123".to_owned()),
                kibana_url: Some("https://kb.example/s/esdiag/app/agent_builder/conversations/conv-123".to_owned()),
            }
        );
        assert!(!failure.retry_safe());
    }

    #[tokio::test]
    async fn interrupted_stream_returns_the_kibana_recovery_location() {
        let stream = concat!(
            "event: conversation_id_set\n",
            "data: {\"data\":{\"conversation_id\":\"conv-orphan\"}}\n\n",
            "event: reasoning\n",
            "data: {\"data\":{\"reasoning\":\"Checking diagnostic\"}}\n\n"
        );
        let (url, _) = serve_sse(stream).await;
        let client = KibanaClient::try_new(url, Auth::None).expect("kibana client");
        let location = AgentBuilderLocation::new(
            Url::parse("https://kb.example").expect("viewer"),
            Some("esdiag".to_owned()),
        );

        let failure = AgentBuilderClient::new(&client, location)
            .ask(
                AgentRequest {
                    agent_id: "elastic-ai-agent".to_owned(),
                    prompt: "Analyze diagnostic abc123".to_owned(),
                    conversation_id: None,
                },
                |_| {},
            )
            .await
            .expect_err("interrupted stream");

        assert_eq!(
            failure,
            AgentFailure::Interrupted {
                conversation_id: Some("conv-orphan".to_owned()),
                kibana_url: Some("https://kb.example/s/esdiag/app/agent_builder/conversations/conv-orphan".to_owned()),
            }
        );
        assert!(!failure.retry_safe());
    }

    async fn serve_sse(stream: &str) -> (Url, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let url = Url::parse(&format!("http://{}", listener.local_addr().expect("address"))).expect("url");
        let request = Arc::new(Mutex::new(String::new()));
        let captured = request.clone();
        let stream = stream.to_owned();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("connection");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = socket.read(&mut buffer).await.expect("read request");
                assert_ne!(read, 0, "connection closed before request completed");
                bytes.extend_from_slice(&buffer[..read]);
                let request = String::from_utf8_lossy(&bytes);
                let Some(headers_end) = request.find("\r\n\r\n") else {
                    continue;
                };
                let length = request[..headers_end]
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or_default();
                if bytes.len() >= headers_end + 4 + length {
                    *captured.lock().await = String::from_utf8_lossy(&bytes).to_string();
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{stream}",
                stream.len()
            );
            socket.write_all(response.as_bytes()).await.expect("write response");
        });
        (url, request)
    }

    async fn serve_json(body: &str) -> (Url, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let url = Url::parse(&format!("http://{}", listener.local_addr().expect("address"))).expect("url");
        let request = Arc::new(Mutex::new(String::new()));
        let captured = request.clone();
        let body = body.to_owned();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("connection");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = socket.read(&mut buffer).await.expect("read request");
                assert_ne!(read, 0, "connection closed before request completed");
                bytes.extend_from_slice(&buffer[..read]);
                if String::from_utf8_lossy(&bytes).contains("\r\n\r\n") {
                    *captured.lock().await = String::from_utf8_lossy(&bytes).to_string();
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.expect("write response");
        });
        (url, request)
    }
}
