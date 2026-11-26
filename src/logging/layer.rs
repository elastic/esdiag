// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use chrono::{SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use std::cell::Cell;
use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::mpsc;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// ECS schema version emitted on every shipped log document.
const ECS_VERSION: &str = "8.11.0";

/// Target prefixes excluded from remote log shipping to prevent transport feedback loops.
const IGNORED_TARGET_PREFIXES: &[&str] = &[
    "esdiag::logging",
    "hyper",
    "hyper_util",
    "reqwest",
    "h2",
    "rustls",
    "tower",
    "elasticsearch",
];

thread_local! {
    static IN_LOG_EXPORTER: Cell<bool> = const { Cell::new(false) };
}

struct ReentrancyGuard;

impl ReentrancyGuard {
    fn enter() -> Option<Self> {
        IN_LOG_EXPORTER.with(|flag| {
            if flag.get() {
                None
            } else {
                flag.set(true);
                Some(Self)
            }
        })
    }
}

impl Drop for ReentrancyGuard {
    fn drop(&mut self) {
        IN_LOG_EXPORTER.with(|flag| flag.set(false));
    }
}

#[derive(Default)]
struct SpanFields {
    fields: Map<String, Value>,
}

struct JsonVisitor {
    message: Option<String>,
    fields: Map<String, Value>,
}

impl JsonVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: Map::new(),
        }
    }

    fn record_value(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = match value {
                Value::String(s) => Some(s),
                other => Some(other.to_string()),
            };
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

impl Visit for JsonVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_value(field, json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, json!(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.record_value(field, json!(value.to_string()));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.record_value(field, json!(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, Value::String(value.to_string()));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_value(field, Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, Value::String(format!("{value:?}")));
    }
}

fn is_ignored_target(target: &str) -> bool {
    IGNORED_TARGET_PREFIXES
        .iter()
        .any(|prefix| target == *prefix || target.starts_with(&format!("{prefix}::")))
}

/// Process-scoped static ECS metadata computed once when the layer is initialized.
struct StaticEcsContext {
    host_name: Option<String>,
    process_pid: u32,
    process_executable: Option<String>,
    process_name: String,
}

impl StaticEcsContext {
    fn capture() -> Self {
        let host_name = detect_host_name();
        let process_pid = std::process::id();
        let process_executable = std::env::current_exe().ok().map(|p| p.display().to_string());
        let process_name = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "esdiag".to_string());
        Self {
            host_name,
            process_pid,
            process_executable,
            process_name,
        }
    }
}

fn detect_host_name() -> Option<String> {
    for key in ["HOSTNAME", "COMPUTERNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    #[cfg(unix)]
    {
        if let Ok(output) = std::process::Command::new("hostname").output()
            && output.status.success()
        {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Non-blocking `tracing_subscriber::Layer` that serializes tracing events into
/// ECS-compliant JSON documents and enqueues them onto a bounded channel.
pub struct ElasticsearchLogLayer {
    tx: mpsc::Sender<Value>,
    dropped: Arc<AtomicU64>,
    context: StaticEcsContext,
}

impl ElasticsearchLogLayer {
    pub(crate) fn new(tx: mpsc::Sender<Value>, dropped: Arc<AtomicU64>) -> Self {
        Self {
            tx,
            dropped,
            context: StaticEcsContext::capture(),
        }
    }
}

impl<S> Layer<S> for ElasticsearchLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::new();
            attrs.record(&mut visitor);
            let mut fields = visitor.fields;
            if let Some(message) = visitor.message {
                fields.insert("message".to_string(), Value::String(message));
            }
            span.extensions_mut().insert(SpanFields { fields });
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = JsonVisitor::new();
            values.record(&mut visitor);
            let mut extensions = span.extensions_mut();
            let entry = extensions.get_mut::<SpanFields>();
            if let Some(span_fields) = entry {
                span_fields.fields.extend(visitor.fields);
                if let Some(message) = visitor.message {
                    span_fields.fields.insert("message".to_string(), Value::String(message));
                }
            } else {
                let mut fields = visitor.fields;
                if let Some(message) = visitor.message {
                    fields.insert("message".to_string(), Value::String(message));
                }
                extensions.insert(SpanFields { fields });
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if is_ignored_target(metadata.target()) {
            return;
        }
        let Some(_guard) = ReentrancyGuard::enter() else {
            return;
        };

        let mut visitor = JsonVisitor::new();
        event.record(&mut visitor);

        // `tracing-log` bridges `log` crate records with `metadata.target() == "log"`
        // and stores the real target/file/line in `log.*` fields.
        let is_log_bridge = metadata.target() == "log";
        let effective_target: std::borrow::Cow<'_, str> = if is_log_bridge {
            visitor
                .fields
                .remove("log.target")
                .and_then(|v| v.as_str().map(str::to_string))
                .map(std::borrow::Cow::Owned)
                .unwrap_or_else(|| std::borrow::Cow::Borrowed(metadata.target()))
        } else {
            std::borrow::Cow::Borrowed(metadata.target())
        };
        if is_ignored_target(&effective_target) {
            return;
        }

        let bridged_file = if is_log_bridge {
            visitor
                .fields
                .remove("log.file")
                .and_then(|v| v.as_str().map(str::to_string))
        } else {
            None
        };
        let bridged_line = if is_log_bridge {
            visitor.fields.remove("log.line").and_then(|v| v.as_u64())
        } else {
            None
        };
        if is_log_bridge {
            visitor.fields.remove("log.module_path");
        }

        let mut labels = Map::new();
        let mut span_names = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                span_names.push(span.name().to_string());
                if let Some(stored) = span.extensions().get::<SpanFields>() {
                    for (k, v) in &stored.fields {
                        labels.insert(k.clone(), value_to_label(v));
                    }
                }
            }
        }

        let mut error_message: Option<String> = None;
        for (k, v) in visitor.fields {
            if matches!(k.as_str(), "error" | "err") {
                error_message = Some(match v {
                    Value::String(s) => s,
                    other => other.to_string(),
                });
            } else {
                labels.insert(k, value_to_label(&v));
            }
        }
        if !span_names.is_empty() {
            labels.insert("span_path".to_string(), Value::String(span_names.join("::")));
        }

        let level = metadata.level().as_str().to_ascii_lowercase();
        let mut log_obj = Map::new();
        log_obj.insert("level".to_string(), Value::String(level));
        log_obj.insert("logger".to_string(), Value::String(effective_target.into_owned()));

        let origin_file = bridged_file.or_else(|| metadata.file().map(str::to_string));
        let origin_line = bridged_line.or_else(|| metadata.line().map(u64::from));
        let origin_function = span_names.last().cloned();
        if origin_file.is_some() || origin_line.is_some() || origin_function.is_some() {
            let mut origin = Map::new();
            if origin_file.is_some() || origin_line.is_some() {
                let mut file_obj = Map::new();
                if let Some(file) = origin_file {
                    file_obj.insert("name".to_string(), Value::String(file));
                }
                if let Some(line) = origin_line {
                    file_obj.insert("line".to_string(), json!(line));
                }
                origin.insert("file".to_string(), Value::Object(file_obj));
            }
            if let Some(func) = origin_function {
                origin.insert("function".to_string(), Value::String(func));
            }
            log_obj.insert("origin".to_string(), Value::Object(origin));
        }

        let mut process_obj = Map::new();
        process_obj.insert("pid".to_string(), json!(self.context.process_pid));
        process_obj.insert("name".to_string(), Value::String(self.context.process_name.clone()));
        if let Some(exe) = &self.context.process_executable {
            process_obj.insert("executable".to_string(), Value::String(exe.clone()));
        }
        if let Some(thread_name) = std::thread::current().name() {
            process_obj.insert("thread".to_string(), json!({ "name": thread_name }));
        }

        let mut doc = Map::new();
        doc.insert(
            "@timestamp".to_string(),
            Value::String(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
        );
        doc.insert("ecs".to_string(), json!({ "version": ECS_VERSION }));
        doc.insert(
            "event".to_string(),
            json!({
                "kind": "event",
                "dataset": "esdiag",
                "module": "esdiag",
            }),
        );
        doc.insert("log".to_string(), Value::Object(log_obj));
        doc.insert(
            "message".to_string(),
            Value::String(visitor.message.unwrap_or_default()),
        );
        doc.insert(
            "service".to_string(),
            json!({
                "name": "esdiag",
                "version": env!("CARGO_PKG_VERSION"),
            }),
        );
        doc.insert("process".to_string(), Value::Object(process_obj));
        if let Some(host_name) = &self.context.host_name {
            doc.insert("host".to_string(), json!({ "name": host_name }));
        }
        if let Some(err_msg) = error_message {
            doc.insert("error".to_string(), json!({ "message": err_msg }));
        }
        if !labels.is_empty() {
            doc.insert("labels".to_string(), Value::Object(labels));
        }

        if let Err(mpsc::error::TrySendError::Full(_)) = self.tx.try_send(Value::Object(doc)) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// ECS `labels.*` values must be flat keyword-compatible scalars (strings).
fn value_to_label(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(s.clone()),
        Value::Bool(b) => Value::String(b.to_string()),
        Value::Number(n) => Value::String(n.to_string()),
        other => Value::String(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::ElasticsearchLogLayer;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    use tokio::sync::mpsc;
    use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt};

    #[test]
    fn serializes_ecs_fields_spans_and_suppresses_transport_targets() {
        let (tx, mut rx) = mpsc::channel(16);
        let dropped = Arc::new(AtomicU64::new(0));
        let layer = ElasticsearchLogLayer::new(tx, Arc::clone(&dropped)).with_filter(EnvFilter::new("debug"));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(target: "esdiag::processor", "process_bundle", bundle = "diag.zip");
            let _enter = span.enter();
            tracing::info!(target: "esdiag::processor", job_id = "abc", count = 3u64, "processing started");
            tracing::event!(target: "log", tracing::Level::DEBUG, log.target = "reqwest::connect", "bridged reqwest noise");
            tracing::debug!(target: "hyper_util::client::legacy::connect::http", "hyper_util noise");
            tracing::debug!(target: "hyper::proto::h1", "internal http noise");
            tracing::debug!(target: "reqwest::connect", "internal connect noise");
            tracing::debug!(target: "esdiag::logging::worker", "internal worker noise");
        });

        let doc = rx.try_recv().expect("expected one serialized log doc");
        assert!(rx.try_recv().is_err(), "transport targets must be suppressed");
        assert_eq!(dropped.load(Ordering::Relaxed), 0);

        assert!(doc["@timestamp"].as_str().is_some_and(|s| s.ends_with('Z')));
        assert_eq!(doc["ecs"]["version"], "8.11.0");
        assert_eq!(doc["event"]["kind"], "event");
        assert_eq!(doc["event"]["dataset"], "esdiag");
        assert_eq!(doc["event"]["module"], "esdiag");
        assert_eq!(doc["log"]["level"], "info");
        assert_eq!(doc["log"]["logger"], "esdiag::processor");
        assert!(doc["log"]["origin"]["file"]["name"].as_str().is_some());
        assert!(doc["log"]["origin"]["file"]["line"].as_u64().is_some());
        assert_eq!(doc["log"]["origin"]["function"], "process_bundle");
        assert_eq!(doc["message"], "processing started");
        assert_eq!(doc["service"]["name"], "esdiag");
        assert_eq!(doc["service"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(doc["process"]["pid"].as_u64().is_some());
        assert_eq!(doc["labels"]["job_id"], "abc");
        assert_eq!(doc["labels"]["count"], "3");
        assert_eq!(doc["labels"]["bundle"], "diag.zip");
        assert_eq!(doc["labels"]["span_path"], "process_bundle");
    }

    #[test]
    fn full_queue_drops_events_without_blocking() {
        let (tx, _rx) = mpsc::channel(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let layer = ElasticsearchLogLayer::new(tx, Arc::clone(&dropped));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "esdiag::processor", "first");
            tracing::info!(target: "esdiag::processor", "second");
            tracing::info!(target: "esdiag::processor", "third");
        });

        assert_eq!(dropped.load(Ordering::Relaxed), 2);
    }
}
