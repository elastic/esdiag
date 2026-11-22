// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The transport-independent MCP tool catalog. Each tool calls the same
//! [`crate::operations`] function as its CLI command and returns the same
//! typed outcome `--format json` emits.

mod params;
#[cfg(test)]
mod tests;

#[cfg(feature = "setup")]
use crate::cli_output::command_failure;
use crate::{
    cli_output::{CliFailure, CliFailureCategory, CommandOutcome, structured_failure},
    data::{Application, HostRole, KnownHostCliUpdate, get_keystore_password},
    job::{
        context::{ExecutionObserver, with_scoped_observer},
        outcome::ExecutionEvent,
    },
    operations,
};
use params::*;
use rmcp::{
    ErrorData, Peer, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Implementation, ProgressNotificationParam, ProgressToken, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use std::{future::Future, pin::pin, str::FromStr, sync::Arc};
use tokio::sync::{Mutex, mpsc};

/// Which tools a transport offers and how their parameters are constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpPolicy {
    /// A local client on the user's machine: the full catalog.
    Local,
}

/// The ESDiag MCP server handler.
#[derive(Clone)]
pub struct EsdiagMcp {
    tool_router: ToolRouter<Self>,
    /// Held while a tool changes hosts, saved jobs, or the output context;
    /// those files are read-modify-write without locking.
    config_writes: Arc<Mutex<()>>,
}

impl EsdiagMcp {
    pub fn new(policy: McpPolicy) -> Self {
        let tool_router = Self::core_tools();
        #[cfg(feature = "keystore")]
        let tool_router = tool_router + Self::job_tools();
        #[cfg(feature = "agent")]
        let tool_router = tool_router + Self::agent_tools();
        #[cfg(feature = "setup")]
        let tool_router = tool_router + Self::setup_tools();
        match policy {
            McpPolicy::Local => {}
        }
        Self {
            tool_router,
            config_writes: Arc::default(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EsdiagMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("esdiag", env!("CARGO_PKG_VERSION")))
    }
}

#[tool_router(router = core_tools)]
impl EsdiagMcp {
    /// List saved hosts, as `esdiag host list`.
    #[tool(annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn host_list(&self) -> CallToolResult {
        outcome_result(operations::host::host_list())
    }

    /// Test authentication for a saved host or resolved template reference, as `esdiag host auth`.
    #[tool(annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn host_auth(&self, Parameters(params): Parameters<HostAuthParams>) -> CallToolResult {
        outcome_result(operations::host::host_auth(params.target).await)
    }

    /// Show the configured Elastic CLI output context, as `esdiag output show`.
    #[tool(annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn output_show(&self) -> CallToolResult {
        outcome_result(operations::output::output_show())
    }

    /// Show local keystore and unlock status, as `esdiag keystore status`.
    #[tool(annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn keystore_status(&self) -> CallToolResult {
        outcome_result(operations::keystore::keystore_status())
    }

    /// Process one diagnostic and export its documents, as `esdiag process`. Reports progress.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn process(
        &self,
        Parameters(params): Parameters<ProcessParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let input = operations::process::ProcessInput {
            input: params.input,
            output: params.output,
            #[cfg(feature = "agent")]
            ask: params.ask,
            account: params.account,
            case: params.case,
            opportunity: params.opportunity,
            user: params.user,
            sources: None,
            scrubbed: params.scrubbed,
            #[cfg(feature = "keystore")]
            save_job: params.save_job,
        };
        if input.writes_stdout() {
            return Ok(failure_result(CliFailure::new(
                CliFailureCategory::InvalidInput,
                "process output '-' writes to stdout, which carries the MCP protocol",
            )));
        }
        #[cfg(feature = "keystore")]
        let _config_write = self.config_write_if(input.save_job.is_some()).await;
        long_running(&context, operations::process::process(input)).await
    }

    /// Collect a diagnostic archive from a saved host, as `esdiag collect`. Reports progress.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn collect(
        &self,
        Parameters(params): Parameters<CollectParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let input = operations::collect::CollectInput {
            host: params.host,
            output: params.output,
            diagnostic_type: params.diagnostic_type,
            include: params.include,
            exclude: params.exclude,
            sources: None,
            account: params.account,
            case: params.case,
            opportunity: params.opportunity,
            user: params.user,
            send_id: params.send,
            #[cfg(feature = "keystore")]
            save_job: params.save_job,
        };
        #[cfg(feature = "keystore")]
        let _config_write = self.config_write_if(input.save_job.is_some()).await;
        long_running(&context, operations::collect::collect(input)).await
    }

    /// Send a raw diagnostic archive to Elastic Upload Service, as `esdiag send`. Reports progress.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn send(
        &self,
        Parameters(params): Parameters<SendParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut progress = Progress::new(&context);
        if let Some(progress) = &mut progress {
            progress.notify("send started".to_string()).await;
        }
        let input = operations::send::SendInput {
            file_name: params.file_name,
            upload_id: params.upload_id,
            api_url: params.api_url,
        };
        let result = tokio::select! {
            result = operations::send::send(input) => result,
            _ = context.ct.cancelled() => return Err(cancelled()),
        };
        if let (Some(progress), Ok(_)) = (&mut progress, &result) {
            progress.notify("send completed".to_string()).await;
        }
        Ok(outcome_result(result))
    }

    /// Add a saved host after validating its connection, as `esdiag host add`. Authentication is only a keystore `secret` reference.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = false
    ))]
    async fn host_add(&self, Parameters(params): Parameters<HostAddParams>) -> CallToolResult {
        let update = match host_update_fields(params.accept_invalid_certs, params.secret, params.roles) {
            Ok(update) => update,
            Err(message) => return failure_result(CliFailure::new(CliFailureCategory::InvalidInput, message)),
        };
        let app = match params.app.as_deref().map(Application::from_str).transpose() {
            Ok(app) => app,
            Err(_) => {
                return failure_result(CliFailure::new(
                    CliFailureCategory::InvalidInput,
                    "app must be elasticsearch, kibana, or logstash",
                ));
            }
        };
        let _config_write = self.config_writes.lock().await;
        outcome_result(
            operations::host::host_add(operations::host::HostAddInput {
                name: params.name,
                app,
                target: params.target,
                url_template: params.url_template,
                update,
                keystore_password: get_keystore_password,
            })
            .await,
        )
    }

    /// Update a saved host, as `esdiag host update`. Authentication is only a keystore `secret` reference.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = false
    ))]
    async fn host_update(&self, Parameters(params): Parameters<HostUpdateParams>) -> CallToolResult {
        let update = match host_update_fields(params.accept_invalid_certs, params.secret, params.roles) {
            Ok(update) => update,
            Err(message) => return failure_result(CliFailure::new(CliFailureCategory::InvalidInput, message)),
        };
        let _config_write = self.config_writes.lock().await;
        outcome_result(
            operations::host::host_update(operations::host::HostUpdateInput {
                name: params.name,
                update,
                keystore_password: get_keystore_password,
            })
            .await,
        )
    }

    /// Validate and persist a named Elastic CLI context as the default output, as `esdiag output set`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn output_set(&self, Parameters(params): Parameters<OutputSetParams>) -> CallToolResult {
        let _config_write = self.config_writes.lock().await;
        outcome_result(operations::output::output_set(params.context))
    }

    /// Remove a saved host, as `esdiag host remove`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = false
    ))]
    async fn host_remove(&self, Parameters(params): Parameters<NameParams>) -> CallToolResult {
        let _config_write = self.config_writes.lock().await;
        outcome_result(operations::host::host_remove(params.name))
    }

    /// Clear the configured Elastic CLI output context, as `esdiag output clear`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn output_clear(&self) -> CallToolResult {
        let _config_write = self.config_writes.lock().await;
        outcome_result(operations::output::output_clear())
    }
}

#[cfg(feature = "keystore")]
#[tool_router(router = job_tools)]
impl EsdiagMcp {
    /// List saved jobs, as `esdiag job list`.
    #[tool(annotations(
        read_only_hint = true,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = false
    ))]
    async fn job_list(&self) -> CallToolResult {
        outcome_result(operations::job::job_list())
    }

    /// Run a saved job, as `esdiag job run`. Jobs that export to stdout are rejected. Reports progress.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn job_run(
        &self,
        Parameters(params): Parameters<NameParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let run = operations::job::job_run(params.name, operations::job::StdoutPolicy::Reject);
        long_running(&context, run).await
    }

    /// Delete a saved job, as `esdiag job delete`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false,
        open_world_hint = false
    ))]
    async fn job_delete(&self, Parameters(params): Parameters<NameParams>) -> CallToolResult {
        let _config_write = self.config_writes.lock().await;
        outcome_result(operations::job::job_delete(params.name))
    }
}

#[cfg(feature = "keystore")]
impl EsdiagMcp {
    /// Serializes a `collect` or `process` call that saves a job, for its
    /// whole run, because the operation writes `jobs.yml` before running.
    async fn config_write_if(&self, writes: bool) -> Option<tokio::sync::MutexGuard<'_, ()>> {
        match writes {
            true => Some(self.config_writes.lock().await),
            false => None,
        }
    }
}

#[cfg(feature = "agent")]
#[tool_router(router = agent_tools)]
impl EsdiagMcp {
    /// Submit one prompt to the configured Kibana Agent Builder agent, as `esdiag agent ask`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true
    ))]
    async fn agent_ask(&self, Parameters(params): Parameters<AgentAskParams>) -> CallToolResult {
        let input = operations::agent::AgentAskInput {
            prompt: params.prompt,
            agent_id: params.agent,
            conversation: params.conversation,
        };
        outcome_result(operations::agent::agent_ask(input, |_, _| {}).await)
    }
}

#[cfg(feature = "setup")]
#[tool_router(router = setup_tools)]
impl EsdiagMcp {
    /// Install assets into a saved output and its linked Kibana viewer, as `esdiag setup output`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn setup_output(&self, Parameters(params): Parameters<SetupOutputParams>) -> CallToolResult {
        setup_result(operations::setup::setup_output(params.name, params.activate_trial).await)
    }

    /// Install Elasticsearch assets into a saved host, as `esdiag setup elasticsearch`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn setup_elasticsearch(&self, Parameters(params): Parameters<SetupHostParams>) -> CallToolResult {
        setup_result(operations::setup::setup_elasticsearch(params.host).await)
    }

    /// Install Kibana assets into a saved host, as `esdiag setup kibana`.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true,
        open_world_hint = true
    ))]
    async fn setup_kibana(&self, Parameters(params): Parameters<SetupHostParams>) -> CallToolResult {
        setup_result(operations::setup::setup_kibana(params.host).await)
    }
}

fn host_update_fields(
    accept_invalid_certs: Option<bool>,
    secret: Option<String>,
    roles: Option<Vec<String>>,
) -> Result<KnownHostCliUpdate, &'static str> {
    let roles = roles
        .map(|roles| {
            roles
                .iter()
                .map(|role| HostRole::from_str(role))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|_| "roles must be collect, send, or view")?;
    Ok(KnownHostCliUpdate {
        accept_invalid_certs,
        secret,
        roles,
        ..KnownHostCliUpdate::default()
    })
}

fn outcome_result(result: eyre::Result<CommandOutcome>) -> CallToolResult {
    match result {
        Ok(outcome) => completed_result(outcome),
        Err(error) => failure_result(structured_failure(&error)),
    }
}

#[cfg(feature = "setup")]
fn setup_result(result: eyre::Result<CommandOutcome>) -> CallToolResult {
    match result {
        Ok(outcome) => completed_result(outcome),
        Err(error) => failure_result(command_failure(&error, true)),
    }
}

fn completed_result(outcome: CommandOutcome) -> CallToolResult {
    let CommandOutcome::Completed(outcome) = outcome else {
        // Tools reject stdout export before calling the operation.
        return failure_result(CliFailure::new(
            CliFailureCategory::Internal,
            "the operation wrote its result to stdout",
        ));
    };
    match serde_json::to_value(&outcome) {
        Ok(value) => CallToolResult::structured(value),
        Err(_) => failure_result(CliFailure::new(
            CliFailureCategory::Internal,
            "the operation result could not be serialized",
        )),
    }
}

fn failure_result(failure: CliFailure) -> CallToolResult {
    let value = serde_json::to_value(&failure).unwrap_or_else(|_| {
        serde_json::json!({
            "result": "command_failed",
            "category": failure.category,
            "message": failure.message,
        })
    });
    CallToolResult::structured_error(value)
}

fn cancelled() -> ErrorData {
    ErrorData::internal_error("request cancelled", None)
}

/// Runs a long operation: forwards its execution events as progress
/// notifications when the request carries a progress token, and drops it when
/// the client cancels the request.
async fn long_running<F>(context: &RequestContext<RoleServer>, operation: F) -> Result<CallToolResult, ErrorData>
where
    F: Future<Output = eyre::Result<CommandOutcome>>,
{
    // Boxed so the operation's large state machine is not moved through each wrapper's stack frame.
    let observed = observed(Progress::new(context), Box::pin(operation));
    tokio::select! {
        result = observed => Ok(outcome_result(result)),
        _ = context.ct.cancelled() => Err(cancelled()),
    }
}

/// Every notification is sent before this returns, so the client receives
/// progress before the tool result.
async fn observed<F: Future>(progress: Option<Progress>, operation: F) -> F::Output {
    let Some(mut progress) = progress else {
        return operation.await;
    };
    let (sender, mut events) = mpsc::unbounded_channel();
    let mut operation = pin!(with_scoped_observer(Arc::new(ProgressObserver { sender }), operation));
    loop {
        tokio::select! {
            output = &mut operation => {
                events.close();
                while let Ok(message) = events.try_recv() {
                    progress.notify(message).await;
                }
                return output;
            }
            Some(message) = events.recv() => progress.notify(message).await,
        }
    }
}

struct ProgressObserver {
    sender: mpsc::UnboundedSender<String>,
}

impl ExecutionObserver for ProgressObserver {
    fn observe(&self, event: &ExecutionEvent) {
        let mut message = format!("{:?} {:?}", event.stage, event.lifecycle).to_lowercase();
        if let Some(detail) = &event.message {
            message.push_str(": ");
            message.push_str(detail);
        }
        let _ = self.sender.send(message);
    }
}

/// Progress notifications for one request: a monotonic event count with no total.
struct Progress {
    peer: Peer<RoleServer>,
    token: ProgressToken,
    count: u32,
}

impl Progress {
    fn new(context: &RequestContext<RoleServer>) -> Option<Self> {
        Some(Self {
            peer: context.peer.clone(),
            token: context.meta.get_progress_token()?,
            count: 0,
        })
    }

    async fn notify(&mut self, message: String) {
        self.count += 1;
        let notification =
            ProgressNotificationParam::new(self.token.clone(), f64::from(self.count)).with_message(message);
        if let Err(error) = self.peer.notify_progress(notification).await {
            tracing::debug!("MCP progress notification failed: {error}");
        }
    }
}
