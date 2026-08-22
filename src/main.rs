#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
mod local;
// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{ArgAction, Args, Parser, Subcommand, builder::BoolishValueParser, builder::styling};
#[cfg(feature = "agent")]
use esdiag::cli_output::{AgentRecovery, AgentSkillTargetResult, AgentSkillsFailureContext, AgentUsageResult};
use esdiag::cli_output::{
    AgentSkillsResult, BundleResult, CliFailure, CliFailureCategory, CliOutcome, CompletedStages, DiagnosticResult,
    FileCounts, IncludedDiagnosticResult, JobInputResult, JobProcessResult, JobSaveResult, JobSendResult, JobStage,
    KeystoreOperation, OutputFormat, ProcessResult, SavedJobResult, SendResult, write_terminal_outcome,
};
use esdiag::job::{FailedStage, JobExecutionFailure, JobOutcome, SavedJobNotFound};
#[cfg(feature = "server")]
use esdiag::server::{AuthProvider, RuntimeMode, Server, ServerStartOptions};
#[cfg(feature = "setup")]
use esdiag::setup;
#[cfg(feature = "agent")]
use esdiag::{
    agent::{
        builder::{AgentBuilderClient, AgentBuilderLocation, AgentFailure, AgentProgress, AgentRequest},
        skills::{EmbeddedSkill, SkillEnvironment, SkillTarget, detected_targets, install},
    },
    client::KibanaClient,
};
use esdiag::{
    client::Client,
    data::{
        Application, HostRole, KnownHost, KnownHostBuilder, KnownHostCliUpdate, OnboardingWorkflow, OutputDeployment,
        SecretAuth, Settings, Uri, add_secret, clear_unlock_lease, collect_application, create_keystore,
        default_unlock_ttl, get_keystore_path, get_password_for_secret_commands, get_unlock_status, keystore_exists,
        list_secret_names, parse_unlock_ttl, remove_secret, resolve_secret_auth, rotate_keystore_password,
        update_secret, validate_existing_keystore_password, write_unlock_lease,
    },
    env::LOG_LEVEL,
    exporter::Exporter,
    onboarding::{
        CollectHostInput, OutputDeploymentInput, inspect as inspect_onboarding, save_collect_host, save_default_job,
        save_default_processing_job, save_output_deployment, save_user, save_workflow,
    },
    processor::{CollectionResult, DiagnosticOutcome, Identifiers, default_collect_archive_name},
    receiver::{
        ElasticCloudAdminRequestError, ElasticsearchRequestError, KibanaRequestError, LogstashRequestError, Receiver,
    },
    uploader,
};
use eyre::{Result, eyre};
use redact::Secret;
use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
    net::Ipv4Addr,
    path::PathBuf,
    process::ExitCode,
    str::FromStr,
    time::Duration,
};
use tracing_subscriber::{EnvFilter, fmt};
use url::Url;

// CLI Styling
const STYLES: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::BrightWhite.on_default())
    .usage(styling::AnsiColor::BrightWhite.on_default())
    .literal(styling::AnsiColor::Green.on_default())
    .placeholder(styling::AnsiColor::Cyan.on_default());
#[cfg(feature = "agent")]
const DEFAULT_AGENT_BUILDER_AGENT: &str = "elastic-ai-agent";

// Define command line arguments
#[derive(Debug, Parser)]
#[command(name = "esdiag", version, styles = STYLES)]
#[command(about = "Elastic Stack Diagnostics (esdiag) - easily collect, share, process and analyze diagnostics", long_about = None)]
struct Cli {
    /// Enable debug logging
    #[arg(global = true, long)]
    debug: bool,
    /// Enable agent-oriented low-noise CLI behavior
    #[arg(long, short = 'a')]
    agent: bool,
    /// Result representation for finite command outcomes
    #[arg(global = true, long, value_enum, default_value_t = OutputFormat::Yaml)]
    format: OutputFormat,
    /// Commands
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory
    Collect {
        /// The host to collect diagnostics from
        #[arg(help = "The Elastic Stack host to collect diagnostics from")]
        host: String,
        /// The output directory to save the diagnostics to
        #[arg(help = "An existing directory to create a diagnostic directory and files in")]
        output: String,
        /// Diagnostic type
        #[arg(
            long,
            default_value = "standard",
            help = "Diagnostic type (minimal, light, standard, support)"
        )]
        r#type: String,
        /// Explicitly include APIs
        #[arg(long, help = "Comma-separated list of APIs to include", value_delimiter = ',')]
        include: Option<Vec<String>>,
        /// Explicitly exclude APIs
        #[arg(long, help = "Comma-separated list of APIs to exclude", value_delimiter = ',')]
        exclude: Option<Vec<String>>,
        /// Override the embedded sources.yml for the detected Elasticsearch or Logstash job.
        /// The file must match the active product or the command fails before collection.
        #[arg(long)]
        sources: Option<String>,
        /// Diagnostic report account name
        #[arg(help = "Diagnostic report account name", long)]
        account: Option<String>,
        /// Case number added to diagnostic report
        #[arg(help = "Diagnostic report case number", long, short)]
        case: Option<String>,
        /// Diagnostic report opportunity
        #[arg(help = "Diagnostic report opportunity", long, short)]
        opportunity: Option<String>,
        /// Diagnostic report user
        #[arg(help = "Diagnostic report user", long = "user", short, value_name = "USER")]
        user: Option<String>,
        /// Elastic Upload Service upload id or URL for immediate upload after collection
        #[arg(
            help = "Elastic Upload Service upload id or URL for immediate upload after collection",
            long = "upload"
        )]
        upload_id: Option<String>,
        /// Save the effective invocation as a named job before continuing execution
        #[cfg(feature = "keystore")]
        #[arg(long = "save-job", value_name = "NAME")]
        save_job: Option<String>,
    },
    /// Start a web server to receive diagnostic bundle uploads
    #[cfg(feature = "server")]
    Serve {
        /// IPv4 address to bind the server to
        #[arg(long, default_value = "0.0.0.0")]
        bind: Ipv4Addr,
        /// The port to bind the server to
        #[arg(help = "The port to bind the server to", long, short, default_value = "2501")]
        port: u16,
        /// Target to send processed diagnostic documents to
        #[arg(
            long_help = "Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD."
        )]
        output: Option<String>,
        /// Web runtime mode for the server
        #[arg(long, value_enum, help = "Web runtime mode: user or service")]
        mode: Option<RuntimeMode>,
        /// Request authentication provider for the server
        #[arg(long, value_enum, help = "Request authentication provider: google-iap or none")]
        auth_provider: Option<AuthProvider>,
        /// Optional comma-separated web feature allowlist (advanced, job-builder)
        #[arg(long, value_name = "FEATURES")]
        web_features: Option<String>,
        /// Kibana URL to display in the web interface
        #[arg(
            long,
            long_help = "Kibana URL to display in the web interface. If not provided, will use the ESDIAG_KIBANA_URL environment variable."
        )]
        kibana: Option<String>,
    },
    /// Manage saved host connections in `~/.esdiag/hosts.yml`
    Host {
        #[command(subcommand)]
        command: HostCommands,
    },
    /// Interactively configure a repeatable local diagnostic workflow
    Init,
    /// Manage a local Elasticsearch, Kibana, and ESDiag deployment
    Local {
        /// Local-stack command and options
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Manage encrypted secrets in the local keystore
    #[command(alias = "secret")]
    Keystore {
        #[command(subcommand)]
        command: KeystoreCommands,
    },
    /// Receives a diagnostic from the input, processes it, and sends processed docs to the output
    Process {
        /// Source to read diagnostic data from
        #[arg(help = "Source to read diagnostic data from (archive, directory, known host or Elastic uploader URL)")]
        input: String,

        /// Target to send processed diagnostic documents to
        #[arg(
            long_help = "Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD."
        )]
        output: Option<String>,

        #[cfg(feature = "agent")]
        /// Ask Agent Builder about the diagnostic after it is processed
        #[arg(long, value_name = "PROMPT")]
        ask: Option<String>,

        /// Diagnostic report account name
        #[arg(help = "Diagnostic report account name", long)]
        account: Option<String>,

        /// Case number added to diagnostic report
        #[arg(help = "Diagnostic report case number", long, short)]
        case: Option<String>,

        /// Diagnostic report opportunity
        #[arg(help = "Diagnostic report opportunity", long, short)]
        opportunity: Option<String>,

        /// Diagnostic report user
        #[arg(help = "Diagnostic report user", long, short)]
        user: Option<String>,
        /// Override the embedded sources.yml for the detected Elasticsearch or Logstash job.
        /// The file must match the active product or the command fails before processing.
        #[arg(long)]
        sources: Option<String>,
        /// Save the effective invocation as a named job before continuing execution
        #[cfg(feature = "keystore")]
        #[arg(long = "save-job", value_name = "NAME")]
        save_job: Option<String>,
    },
    /// Upload a raw diagnostic archive to Elastic Upload Service
    Upload {
        /// Local diagnostic archive to upload
        #[arg(help = "Local diagnostic archive file path")]
        file_name: String,
        /// Elastic Upload Service upload id or URL
        #[arg(help = "Upload id or Elastic Upload Service URL")]
        upload_id: String,
        /// Upload API base URL
        #[arg(
            long,
            default_value = uploader::DEFAULT_UPLOAD_API_URL,
            help = "Elastic Upload Service base URL"
        )]
        api_url: String,
    },
    #[cfg(feature = "setup")]
    /// Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host
    Setup {
        /// Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked.
        #[arg(
            help = "Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked."
        )]
        host: Option<String>,
    },
    #[cfg(feature = "agent")]
    /// Ask Kibana Agent Builder or install the local ESDiag skill
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Manage saved diagnostic jobs
    #[cfg(feature = "keystore")]
    Job {
        #[command(subcommand)]
        command: JobCommands,
    },
}

#[cfg(feature = "agent")]
#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// Submit one prompt to the configured Kibana Agent Builder agent
    Ask {
        /// Prompt passed unchanged to Agent Builder
        prompt: String,
        /// Agent Builder agent identifier
        #[arg(long = "agent", default_value = DEFAULT_AGENT_BUILDER_AGENT)]
        agent_id: String,
        /// Continue this Kibana Agent Builder conversation
        #[arg(long, conflicts_with = "new")]
        conversation: Option<String>,
        /// Explicitly start a new Kibana Agent Builder conversation
        #[arg(long, conflicts_with = "conversation")]
        new: bool,
    },
    /// Install the embedded ESDiag skill into supported coding agents
    Skills {
        /// Install into this user-scoped coding agent target
        #[arg(long, value_enum)]
        target: Vec<AgentSkillTarget>,
        /// Replace locally modified or unrecognized ESDiag skill directories
        #[arg(long)]
        force: bool,
    },
}

#[cfg(feature = "agent")]
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum AgentSkillTarget {
    Claude,
    Codex,
    #[value(name = "opencode")]
    OpenCode,
}

#[cfg(feature = "agent")]
impl From<AgentSkillTarget> for SkillTarget {
    fn from(target: AgentSkillTarget) -> Self {
        match target {
            AgentSkillTarget::Claude => Self::Claude,
            AgentSkillTarget::Codex => Self::Codex,
            AgentSkillTarget::OpenCode => Self::OpenCode,
        }
    }
}

#[cfg(feature = "keystore")]
#[derive(Debug, Subcommand)]
enum JobCommands {
    /// Run a saved job by name
    Run {
        /// Name of the saved job to run
        name: String,
    },
    /// List all saved jobs
    List,
    /// Delete a saved job by name
    Delete {
        /// Name of the saved job to delete
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum HostCommands {
    /// Add a saved host
    Add {
        /// A name to identify this host
        #[arg(help = "A name to identify this host")]
        name: String,
        /// A concrete URL or resolved template reference
        #[arg(help = "A concrete URL, a template definition target, or a resolved template reference")]
        target: String,
        /// Application of this host when the target cannot infer it
        #[arg(help = "Application of this host when the target cannot infer it", long)]
        app: Option<Application>,
        /// Treat <target> as a saved host URL template instead of a concrete URL
        #[arg(help = "Persist <target> as a saved host URL template", long, action = ArgAction::SetTrue)]
        url_template: bool,
        #[command(flatten)]
        args: HostMutationArgs,
    },
    /// Update an existing saved host
    Update {
        /// Name of the saved host to update
        name: String,
        #[command(flatten)]
        args: HostMutationArgs,
    },
    /// Remove an existing saved host
    Remove {
        /// Name of the saved host to remove
        name: String,
    },
    /// List all saved hosts
    List,
    /// Test authentication for a saved host
    Auth {
        /// Saved host name or resolved template reference to test
        target: String,
    },
    #[command(external_subcommand)]
    Legacy(Vec<String>),
}

#[derive(Debug, Args, Clone)]
struct HostMutationArgs {
    /// Accept invalid certificates
    #[arg(
        help = "Accept invalid certificates",
        long,
        value_parser = BoolishValueParser::new()
    )]
    accept_invalid_certs: Option<bool>,
    /// ApiKey for authentication
    #[arg(
        help = "ApiKey, passed as http header",
        long,
        short = 'k',
        conflicts_with_all = &["username", "password"]
    )]
    apikey: Option<String>,
    /// Username for authentication
    #[arg(
        help = "Username for authentication",
        long = "user",
        visible_alias = "username",
        short
    )]
    username: Option<String>,
    /// Password for authentication
    #[arg(help = "Password for authentication", long, short)]
    password: Option<String>,
    /// Secret identifier in the encrypted keystore
    #[arg(
        help = "Secret identifier in the encrypted keystore",
        long,
        conflicts_with_all = &["apikey", "username", "password"]
    )]
    secret: Option<String>,
    /// Comma-separated host roles (collect,send,view)
    #[arg(help = "Comma-separated host roles", long, value_delimiter = ',')]
    roles: Option<Vec<HostRole>>,
}

impl From<HostMutationArgs> for KnownHostCliUpdate {
    fn from(value: HostMutationArgs) -> Self {
        Self {
            accept_invalid_certs: value.accept_invalid_certs,
            apikey: value.apikey.map(Secret::new),
            password: value.password.map(Secret::new),
            roles: value.roles,
            secret: value.secret,
            username: value.username,
        }
    }
}

#[derive(Debug, Subcommand)]
enum KeystoreCommands {
    /// Add a secret to the encrypted keystore
    Add {
        /// Secret identifier
        secret_id: String,
        /// Username for authentication
        #[arg(
            help = "Username for authentication",
            long = "user",
            visible_alias = "username",
            short
        )]
        username: Option<String>,
        /// Password for authentication
        #[arg(
            help = "Password for authentication (prompts when omitted in interactive shells)",
            long,
            short,
            num_args = 0..=1,
            default_missing_value = ""
        )]
        password: Option<String>,
        /// ApiKey for authentication
        #[arg(
            help = "ApiKey, passed as http header (prompts when omitted in interactive shells)",
            long,
            short = 'k',
            num_args = 0..=1,
            default_missing_value = "",
            conflicts_with_all = &["username", "password"]
        )]
        apikey: Option<String>,
    },
    /// Update an existing secret in the encrypted keystore
    Update {
        /// Secret identifier
        secret_id: String,
        /// Username for authentication
        #[arg(
            help = "Username for authentication",
            long = "user",
            visible_alias = "username",
            short
        )]
        username: Option<String>,
        /// Password for authentication
        #[arg(
            help = "Password for authentication (prompts when omitted in interactive shells)",
            long,
            short,
            num_args = 0..=1,
            default_missing_value = ""
        )]
        password: Option<String>,
        /// ApiKey for authentication
        #[arg(
            help = "ApiKey, passed as http header (prompts when omitted in interactive shells)",
            long,
            short = 'k',
            num_args = 0..=1,
            default_missing_value = "",
            conflicts_with_all = &["username", "password"]
        )]
        apikey: Option<String>,
    },
    /// Remove a secret from the encrypted keystore
    Remove {
        /// Secret identifier
        secret_id: String,
        /// Username for authentication
        #[arg(
            help = "Username for authentication",
            long = "user",
            visible_alias = "username",
            short
        )]
        username: Option<String>,
        /// Password for authentication
        #[arg(help = "Password for authentication", long, short)]
        password: Option<String>,
        /// ApiKey for authentication
        #[arg(
            help = "ApiKey, passed as http header",
            long,
            short = 'k',
            conflicts_with_all = &["username", "password"]
        )]
        apikey: Option<String>,
    },
    /// Unlock the local keystore for future CLI runs
    Unlock {
        /// Unlock duration like 90m, 24h, or 7d
        #[arg(long, help = "Unlock duration like 90m, 24h, or 7d")]
        ttl: Option<String>,
    },
    /// Lock the local keystore for future CLI runs
    Lock,
    /// Show local keystore and unlock status
    Status,
    /// Change the keystore password
    Password,
    /// Migrate legacy host credentials in hosts.yml into the keystore
    Migrate,
}

const TOKIO_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TOKIO_THREAD_STACK_SIZE)
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Failed to create runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(async_main()) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("{error:?}");
            ExitCode::FAILURE
        }
    }
}

async fn async_main() -> Result<ExitCode> {
    // Parse CLI early to configure execution mode and logging.
    let cli = Cli::parse();
    let filter = resolve_tracing_filter(&cli);
    init_tracing(filter);
    let stdout_owned = command_owns_stdout(&cli);
    let no_command = cli.command.is_none();

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        tracing::debug!("{:?}", panic);
        tracing::error!("{}", panic);
    }));

    clear_last_run_files()?;

    let format = cli.format;
    match run(cli, format).await {
        Ok(result) => {
            if let Some(outcome) = result.outcome {
                write_terminal_outcome(format, &outcome)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            tracing::error!("{}", e);
            if !stdout_owned && !no_command {
                let failure = structured_failure(&e);
                write_terminal_outcome(format, &failure)?;
                return Ok(ExitCode::FAILURE);
            }
            Err(eyre!(e))
        }
    }
}

fn init_tracing(filter: EnvFilter) {
    // Bridge `log` records from dependencies when available, but tolerate hosts that
    // already installed a global logger before invoking this binary.
    if let Err(err) = tracing_log::LogTracer::init() {
        eprintln!("tracing log bridge already initialized: {err}");
    }

    let subscriber = fmt().with_env_filter(filter).with_writer(std::io::stderr).finish();
    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("tracing subscriber already initialized: {err}");
    }
}

#[derive(Debug)]
struct CommandResult {
    outcome: Option<CliOutcome>,
}

#[cfg(feature = "agent")]
#[derive(Debug)]
struct ProcessAskStdoutConflict;

#[cfg(feature = "agent")]
impl std::fmt::Display for ProcessAskStdoutConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "--ask cannot be used when process output is '-' because processed documents own stdout"
        )
    }
}

#[cfg(feature = "agent")]
impl std::error::Error for ProcessAskStdoutConflict {}

impl CommandResult {
    fn outcome(outcome: CliOutcome) -> Self {
        Self { outcome: Some(outcome) }
    }

    fn stream() -> Self {
        Self { outcome: None }
    }
}

fn is_agent_mode(cli: &Cli) -> bool {
    cli.agent || std::env::var_os("CLAUDECODE").is_some()
}

fn command_owns_stdout(cli: &Cli) -> bool {
    #[cfg(feature = "agent")]
    let direct_process_stream = matches!(
        &cli.command,
        Some(Commands::Process {
            output: Some(output),
            ask: None,
            ..
        }) if output == "-"
    );
    #[cfg(not(feature = "agent"))]
    let direct_process_stream = matches!(
        &cli.command,
        Some(Commands::Process {
            output: Some(output),
            ..
        }) if output == "-"
    );
    #[cfg(feature = "keystore")]
    let saved_job_stream = matches!(
        &cli.command,
        Some(Commands::Job {
            command: JobCommands::Run { name }
        }) if esdiag::data::load_saved_jobs()
            .ok()
            .and_then(|jobs| jobs.get(name).cloned())
            .and_then(|job| {
                job.process()
                    .map(|process| process.export == esdiag::job::model::ExportTarget::Stdout)
            })
            .unwrap_or(false)
    );
    #[cfg(not(feature = "keystore"))]
    let saved_job_stream = false;
    let local_raw_stdout = matches!(
        &cli.command,
        Some(Commands::Local { args })
            if args
                .first()
                .and_then(|argument| argument.to_str())
                .map(|command| matches!(command, "help" | "--help" | "-h" | "version" | "--version" | "secrets"))
                .unwrap_or(true)
    );
    direct_process_stream || saved_job_stream || local_raw_stdout
}

fn resolve_tracing_filter(cli: &Cli) -> EnvFilter {
    if cli.debug {
        EnvFilter::new("debug")
    } else if is_agent_mode(cli) {
        EnvFilter::new("warn")
    } else {
        EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new(LOG_LEVEL))
    }
}

async fn run_local_lifecycle(args: Vec<OsString>) -> Result<CommandResult> {
    let raw_stdout = args
        .first()
        .and_then(|argument| argument.to_str())
        .map(|command| matches!(command, "help" | "--help" | "-h" | "version" | "--version" | "secrets"))
        .unwrap_or(true);
    let outcome = local::run(&args).await?;
    Ok(if raw_stdout {
        CommandResult::stream()
    } else {
        CommandResult::outcome(outcome)
    })
}

fn classify_failure(error: &eyre::Report) -> CliFailureCategory {
    if let Some(response) = http_response_details(error) {
        return match response.status {
            401 | 403 => CliFailureCategory::AuthenticationFailed,
            404 => CliFailureCategory::NotFound,
            500..=599 => CliFailureCategory::Internal,
            _ => CliFailureCategory::InvalidInput,
        };
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("not found") {
        CliFailureCategory::NotFound
    } else if message.contains("auth") || message.contains("credential") {
        CliFailureCategory::AuthenticationFailed
    } else if message.contains("collect") {
        CliFailureCategory::CollectionFailed
    } else if message.contains("process") {
        CliFailureCategory::ProcessingFailed
    } else if message.contains("upload") || message.contains("send") {
        CliFailureCategory::SendFailed
    } else if message.contains("keystore") || message.contains("secret") {
        CliFailureCategory::KeystoreFailed
    } else {
        CliFailureCategory::InvalidInput
    }
}

fn safe_failure_message(error: &eyre::Report) -> String {
    if let Some(response) = http_response_details(error) {
        return match response.status {
            401 => "The server rejected the request because authentication credentials are required.".to_string(),
            403 => "The server rejected the request because the credentials are not authorized.".to_string(),
            404 => "The requested server resource was not found.".to_string(),
            500..=599 => format!(
                "The server failed to complete the request with HTTP {}.",
                response.status
            ),
            status => format!("The server rejected the request with HTTP {status}."),
        };
    }
    match classify_failure(error) {
        CliFailureCategory::NotFound => "requested resource was not found",
        CliFailureCategory::AuthenticationFailed => "authentication failed",
        CliFailureCategory::CollectionFailed => "diagnostic collection failed",
        CliFailureCategory::ProcessingFailed => "diagnostic processing failed",
        CliFailureCategory::SendFailed => "diagnostic upload failed",
        CliFailureCategory::KeystoreFailed => "keystore operation failed",
        CliFailureCategory::SetupFailed => "setup failed",
        CliFailureCategory::Internal | CliFailureCategory::InvalidInput => "command execution failed",
    }
    .to_string()
}

struct HttpResponseDetails {
    status: u16,
    error_type: Option<String>,
    reason: Option<String>,
}

fn parse_http_response_details(status: u16, body: &str) -> HttpResponseDetails {
    let response_error = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|body| body.get("error").unwrap_or(&body).clone());
    let error_type = response_error
        .as_ref()
        .and_then(|error| error.get("type"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let reason = response_error
        .as_ref()
        .and_then(|error| error.get("reason"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    HttpResponseDetails {
        status,
        error_type,
        reason,
    }
}

fn http_response_details(error: &eyre::Report) -> Option<HttpResponseDetails> {
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<ElasticsearchRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<KibanaRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<LogstashRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
        if let Some(error) = cause.downcast_ref::<ElasticCloudAdminRequestError>() {
            return Some(parse_http_response_details(error.status.as_u16(), &error.body));
        }
    }
    None
}

fn structured_failure(error: &eyre::Report) -> CliFailure {
    #[cfg(feature = "agent")]
    if let Some(conflict) = error.downcast_ref::<ProcessAskStdoutConflict>() {
        return CliFailure::new(CliFailureCategory::InvalidInput, conflict.to_string());
    }
    if let Some(missing_job) = error.downcast_ref::<SavedJobNotFound>() {
        return CliFailure::new(CliFailureCategory::NotFound, "saved job was not found")
            .resource(missing_job.name.clone());
    }
    #[cfg(feature = "agent")]
    if let Some(agent_failure) = error.downcast_ref::<AgentFailure>() {
        let category = match agent_failure {
            AgentFailure::Http { status: 401 | 403 } => CliFailureCategory::AuthenticationFailed,
            AgentFailure::Http { status: 404 } => CliFailureCategory::NotFound,
            AgentFailure::Http { status: 500..=599 } | AgentFailure::Remote | AgentFailure::Protocol { .. } => {
                CliFailureCategory::Internal
            }
            AgentFailure::Http { .. } | AgentFailure::Transport | AgentFailure::Interrupted { .. } => {
                CliFailureCategory::InvalidInput
            }
        };
        let mut failure = CliFailure::new(category, agent_failure.to_string());
        if let AgentFailure::Http { status } = agent_failure {
            failure = failure.http_status(*status);
        }
        if let AgentFailure::Interrupted {
            conversation_id,
            kibana_url,
        } = agent_failure
        {
            failure = failure.recovery(AgentRecovery {
                conversation_id: conversation_id.clone(),
                kibana_url: kibana_url.clone(),
                retry_safe: agent_failure.retry_safe(),
            });
        }
        return failure;
    }
    #[cfg(feature = "agent")]
    if let Some(skill_failure) = error.downcast_ref::<SkillInstallationFailure>() {
        return CliFailure::new(CliFailureCategory::Internal, skill_failure.to_string())
            .target_results(skill_failure.context.results.clone())
            .agent_skills(skill_failure.context.clone());
    }
    let Some(job_failure) = error.downcast_ref::<JobExecutionFailure>() else {
        let mut failure = CliFailure::new(classify_failure(error), safe_failure_message(error));
        if let Some(response) = http_response_details(error) {
            failure = failure.http_status(response.status);
            if let Some(error_type) = response.error_type {
                failure = failure.type_(error_type);
            }
            if let Some(reason) = response.reason {
                failure = failure.reason(reason);
            }
        }
        return failure;
    };
    let (category, failed_stage) = match job_failure.stage {
        FailedStage::Collect => (CliFailureCategory::CollectionFailed, JobStage::Collect),
        FailedStage::Process => (CliFailureCategory::ProcessingFailed, JobStage::Process),
        FailedStage::Send => (CliFailureCategory::SendFailed, JobStage::Send),
    };
    let outcome = &job_failure.outcome;
    let completed = CompletedStages {
        save: outcome
            .bundle_path
            .as_ref()
            .filter(|_| outcome.bundle_retained && outcome.bundle_created)
            .map(|path| BundleResult {
                path: path.display().to_string(),
            }),
        process: outcome.execution.as_ref().and_then(execution_process_result),
        send: outcome.upload_slug.as_ref().map(|slug| SendResult {
            destination: format!("https://upload.elastic.co/g/{slug}"),
        }),
    };
    let mut failure = CliFailure::new(category, safe_failure_message(error));
    failure.failed_stage = Some(failed_stage);
    failure.retry_safe = Some(false);
    if completed.save.is_some() || completed.process.is_some() || completed.send.is_some() {
        failure.completed = Some(completed);
    }
    failure
}

fn collection_outcome(result: CollectionResult, upload_destination: Option<String>) -> CliOutcome {
    CliOutcome::ArchiveCollected {
        path: result.path,
        files: FileCounts {
            successful: result.success,
            total: result.total,
        },
        duration_ms: Some(result.duration_ms),
        upload_destination,
    }
}

fn format_execution_failure(outcome: &esdiag::job::outcome::ExecutionOutcome) -> String {
    let failures = outcome
        .stages
        .iter()
        .filter_map(|stage| match &stage.status {
            esdiag::job::outcome::StageStatus::Failed(error) => Some(format!("{:?} failed: {error}", stage.stage)),
            esdiag::job::outcome::StageStatus::Blocked(reason) => Some(format!("{:?} blocked: {reason}", stage.stage)),
            esdiag::job::outcome::StageStatus::Succeeded | esdiag::job::outcome::StageStatus::Skipped(_) => None,
        })
        .collect::<Vec<_>>();
    if failures.is_empty() {
        "Job did not complete successfully".to_string()
    } else {
        failures.join("\n")
    }
}

fn execution_process_result(outcome: &esdiag::job::outcome::ExecutionOutcome) -> Option<ProcessResult> {
    let report = outcome.report.as_ref()?;
    let included = outcome
        .children
        .iter()
        .map(|child| match (child.diagnostic_outcome, child.report()) {
            (DiagnosticOutcome::Skipped(_), _) => IncludedDiagnosticResult::Skipped {
                source: child.path.clone(),
                product: Some(esdiag::processor::display_label(child.application(), child.platform())),
                reason: child
                    .execution_error()
                    .unwrap_or("diagnostic processing skipped")
                    .to_string(),
            },
            (_, Some(report)) => IncludedDiagnosticResult::Completed {
                source: child.path.clone(),
                diagnostic: DiagnosticResult {
                    id: report.diagnostic.metadata.id.clone(),
                    product: report.diagnostic.display_label(),
                    documents: report.diagnostic.docs.created,
                    duration_ms: child.runtime.unwrap_or_default(),
                    source: child.path.clone(),
                    output: String::new(),
                    kibana_url: report.diagnostic.kibana_link.clone(),
                },
            },
            (_, None) => IncludedDiagnosticResult::Failed {
                source: child.path.clone(),
                error: child
                    .execution_error()
                    .unwrap_or("included diagnostic processing failed")
                    .to_string(),
            },
        })
        .collect();

    Some(ProcessResult {
        diagnostic: DiagnosticResult {
            id: report.diagnostic.metadata.id.clone(),
            product: report.diagnostic.display_label(),
            documents: report.diagnostic.docs.created,
            duration_ms: report.diagnostic.processing_duration,
            source: "primary".to_string(),
            output: String::new(),
            kibana_url: report.diagnostic.kibana_link.clone(),
        },
        included,
    })
}

fn job_outcome(name: String, outcome: JobOutcome) -> CliOutcome {
    CliOutcome::JobCompleted {
        job: name,
        save: outcome
            .bundle_path
            .filter(|_| outcome.bundle_retained && outcome.bundle_created)
            .map(|path| BundleResult {
                path: path.display().to_string(),
            }),
        process: outcome.execution.as_ref().and_then(execution_process_result),
        send: outcome.upload_slug.map(|slug| SendResult {
            destination: format!("https://upload.elastic.co/g/{slug}"),
        }),
    }
}

fn host_result(name: String, host: &KnownHost) -> esdiag::cli_output::HostResult {
    esdiag::cli_output::HostResult {
        name,
        app: host.app().map(|app| app.key().to_string()),
        roles: host.roles().iter().map(ToString::to_string).collect(),
        target: host.transport_display(),
        secret_reference: host.secret_reference().map(str::to_string),
    }
}

fn saved_job_result(name: String, job: &esdiag::data::Job) -> SavedJobResult {
    let input = match job.input() {
        esdiag::job::model::Input::Collect {
            host, diagnostic_type, ..
        } => JobInputResult::Collect {
            host: host.clone(),
            diagnostic_type: diagnostic_type.clone(),
        },
        esdiag::job::model::Input::CollectBinding {
            binding,
            diagnostic_type,
            ..
        } => JobInputResult::Collect {
            host: format!("binding:{}", binding.as_str()),
            diagnostic_type: diagnostic_type.clone(),
        },
        esdiag::job::model::Input::Load { uri } => JobInputResult::Load {
            source: uri.to_string(),
        },
        esdiag::job::model::Input::LoadBinding { binding } => JobInputResult::Load {
            source: format!("binding:{}", binding.as_str()),
        },
    };
    let save = job.save().map(|save| JobSaveResult {
        directory: save.dir.as_ref().map(|path| path.display().to_string()),
    });
    let process = job.process().map(|process| JobProcessResult {
        export: match &process.export {
            esdiag::job::model::ExportTarget::KnownHost { name } => format!("host:{name}"),
            esdiag::job::model::ExportTarget::File { path } => path.display().to_string(),
            esdiag::job::model::ExportTarget::Directory { output_dir } => output_dir.display().to_string(),
            esdiag::job::model::ExportTarget::Stdout => "-".to_string(),
            esdiag::job::model::ExportTarget::Binding { binding } => format!("binding:{}", binding.as_str()),
        },
    });
    let send = job.send().map(|send| JobSendResult {
        upload_id: send.upload_id.clone(),
    });
    SavedJobResult {
        name,
        input,
        save,
        process,
        send,
    }
}

#[tracing::instrument(skip_all)]
async fn run(cli: Cli, format: OutputFormat) -> Result<CommandResult> {
    // If there are CLI arguments but no subcommand, avoid starting the desktop/Tauri
    // entrypoint. The desktop UI should only start when launched absolutely without arguments.
    if should_error_for_missing_subcommand(std::env::args_os().len(), cli.command.is_none()) {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        cmd.print_help()?;
        return Err(eyre!("No subcommand provided. Use --help for usage information."));
    }

    if let Some(command) = cli.command {
        match command {
            #[cfg(feature = "server")]
            Commands::Serve {
                bind,
                port,
                output,
                mode,
                auth_provider,
                web_features,
                kibana,
            } => {
                tracing::info!("Starting ESDiag server");
                let runtime_mode = resolve_serve_runtime_mode(mode)?;
                let exporter = resolve_serve_exporter(output)?;
                let exporter_owns_stdout = exporter.target_uri() == "stdio://stdout";

                let kibana_url = kibana.unwrap_or_else(|| {
                    esdiag::env::get_string("ESDIAG_KIBANA_URL")
                        .map(|url| esdiag::env::append_kibana_space(&url))
                        .unwrap_or_else(|_| "http://localhost:5601".to_string())
                });

                let (mut server, bound_addr) = Server::start_with_options(
                    bind.octets(),
                    port,
                    exporter,
                    kibana_url,
                    runtime_mode,
                    ServerStartOptions {
                        auth_provider,
                        web_features: web_features.as_deref(),
                        ..ServerStartOptions::default()
                    },
                )
                .await?;

                if exporter_owns_stdout {
                    tracing::info!("Server ready at {bound_addr}");
                } else {
                    write_terminal_outcome(
                        format,
                        &CliOutcome::ServerReady {
                            address: bound_addr.ip().to_string(),
                            port: bound_addr.port(),
                            runtime_mode: runtime_mode.to_string(),
                            output: "configured".to_string(),
                        },
                    )?;
                }
                wait_for_shutdown_signal().await?;

                server.shutdown().await;
                Ok(CommandResult::stream())
            }
            Commands::Collect {
                host,
                output,
                r#type,
                include,
                exclude,
                sources,
                account,
                case,
                opportunity,
                user,
                upload_id,
                #[cfg(feature = "keystore")]
                save_job,
            } => {
                #[cfg(feature = "keystore")]
                if let Some(name) = save_job.as_deref() {
                    let identifiers =
                        Identifiers::new(account.clone(), case.clone(), None, opportunity.clone(), user.clone());
                    let job = derive_collect_job(&host, &output, &r#type, upload_id.as_deref(), identifiers)?;
                    esdiag::job::save_job(name, job)?;
                }
                let known_host = Uri::try_from(host)?;
                let output = Uri::try_from(output)?;
                match known_host {
                    Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
                        ensure_host_role(&host, HostRole::Collect, "collect")?;
                        let application = collect_application(host.app())?;
                        if let Some(sources) = sources {
                            esdiag::processor::init_sources(sources_application_key(application)?, sources)?;
                        }
                        tracing::info!("Collecting diagnostic from {host}");
                        tracing::info!("Saving diagnostic to {output}");
                        let output_dir = match output {
                            Uri::Directory(path) | Uri::File(path) => path,
                            _ => {
                                return Err(eyre!("Collect output must be a local directory path"));
                            }
                        };
                        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
                        let filename = format!("{}.zip", default_collect_archive_name(application, &timestamp));
                        let identifiers = Identifiers::new(account, case, Some(filename), opportunity, user);
                        let binding = esdiag::job::model::BindingKey::try_new("cli-collect-input")?;
                        let mut context = esdiag::job::context::ExecutionContext::default();
                        context
                            .inputs
                            .bind_receiver(binding.clone(), Receiver::try_from(host)?, Some(application));
                        let job = esdiag::job::model::Job::try_new(
                            identifiers,
                            esdiag::job::model::Input::CollectBinding {
                                binding,
                                diagnostic_type: r#type,
                                include,
                                exclude,
                            },
                            Some(esdiag::job::model::SaveTarget::retained(output_dir)),
                            None,
                            upload_id.map(|upload_id| esdiag::job::model::SendTarget { upload_id }),
                        )?;
                        let outcome = esdiag::job::executor::execute_with_context(job, context).await;
                        if !outcome.succeeded() {
                            return Err(eyre!("{}", format_execution_failure(&outcome)));
                        }
                        let result = outcome
                            .collection
                            .ok_or_else(|| eyre!("Collection completed without a collection result"))?;
                        let upload_destination = outcome
                            .upload
                            .as_ref()
                            .map(|upload| format!("https://upload.elastic.co/g/{}", upload.slug));
                        Ok(CommandResult::outcome(collection_outcome(result, upload_destination)))
                    }
                    Uri::ElasticCloud(_) => Err(eyre!("Elastic Cloud API collection not yet implemented")),
                    _ => Err(eyre!("Collect requires a known host")),
                }
            }
            Commands::Init => run_init_wizard().await,
            Commands::Local { args } => run_local_lifecycle(args).await,
            #[cfg(feature = "agent")]
            Commands::Agent { command } => match command {
                AgentCommands::Ask {
                    prompt,
                    agent_id,
                    conversation,
                    new,
                } => {
                    let _ = new;
                    run_agent_ask(prompt, agent_id, conversation).await
                }
                AgentCommands::Skills { target, force } => run_agent_skills(target, force),
            },
            Commands::Host { command } => match command {
                HostCommands::Add {
                    name,
                    app,
                    target,
                    url_template,
                    args,
                } => {
                    tracing::info!("Adding host {name}");
                    if KnownHost::get_known(&name).is_some() {
                        return Err(eyre!("Host '{name}' already exists"));
                    }
                    let mut update = build_host_cli_update(args);
                    let secret_auth = if update.secret.is_some() {
                        resolve_host_secret_auth(update.secret.as_deref())?
                    } else if url_template {
                        match resolve_same_name_host_secret_auth(&name)? {
                            Some(secret_auth) => {
                                tracing::debug!("Using host name {} as secret_id", name);
                                update.secret = Some(name.clone());
                                Some(secret_auth)
                            }
                            None => None,
                        }
                    } else {
                        None
                    };
                    let host = if url_template {
                        build_host_from_definition(app, &target, true, &update, secret_auth)?
                    } else if let Some(host) = maybe_materialize_template_target(&target)? {
                        host.merge_cli_update(&update, secret_auth)?
                    } else {
                        build_host_from_definition(app, &target, false, &update, secret_auth)?
                    };
                    save_host(&name, host.clone(), "added", !host.is_template()).await?;
                    Ok(CommandResult::outcome(CliOutcome::HostAdded {
                        host: host_result(name, &host),
                    }))
                }
                HostCommands::Update { name, args } => {
                    tracing::info!("Updating host {name}");
                    let update = build_host_cli_update(args);
                    if update.is_empty() {
                        return Err(eyre!(
                            "No host update fields were provided. Use `esdiag host auth {name}` to test the saved host without modifying it."
                        ));
                    }
                    let existing = KnownHost::get_known(&name).ok_or_else(|| eyre!("Host '{name}' not found"))?;
                    let secret_auth = if update.secret.is_some() {
                        resolve_host_secret_auth(update.secret.as_deref())?
                    } else {
                        None
                    };
                    let host = existing.merge_cli_update(&update, secret_auth)?;
                    save_host(&name, host.clone(), "updated", !host.is_template()).await?;
                    Ok(CommandResult::outcome(CliOutcome::HostUpdated {
                        host: host_result(name, &host),
                    }))
                }
                HostCommands::Remove { name } => {
                    tracing::info!("Removing host {name}");
                    let hostfile = delete_host_from_cli(&name)?;
                    tracing::info!("Host {name} successfully deleted from {hostfile}");
                    Ok(CommandResult::outcome(CliOutcome::HostRemoved { name, path: hostfile }))
                }
                HostCommands::List => {
                    let hosts = KnownHost::parse_hosts_yml()?
                        .into_iter()
                        .map(|(name, host)| host_result(name, &host))
                        .collect();
                    Ok(CommandResult::outcome(CliOutcome::HostsListed { hosts }))
                }
                HostCommands::Auth { target } => {
                    tracing::info!("Testing saved host {target}");
                    if let Some(host) = KnownHost::resolve_template_reference(&target)? {
                        validate_host_connection(&target, Uri::try_from(host)?).await?;
                        return Ok(CommandResult::outcome(CliOutcome::HostAuthenticated {
                            name: target,
                            message: None,
                        }));
                    }
                    let host = KnownHost::get_known(&target).ok_or_else(|| eyre!("Host '{target}' not found"))?;
                    if host.is_template() {
                        return Ok(CommandResult::outcome(CliOutcome::HostAuthenticated {
                            name: target.clone(),
                            message: Some(KnownHost::template_guidance(&target)),
                        }));
                    }
                    let uri = Uri::try_from(host)?;
                    validate_host_connection(&target, uri).await?;
                    Ok(CommandResult::outcome(CliOutcome::HostAuthenticated {
                        name: target,
                        message: None,
                    }))
                }
                HostCommands::Legacy(args) => Err(legacy_host_command_error(&args)),
            },
            Commands::Keystore { command } => match command {
                KeystoreCommands::Add {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let (username, password, apikey) = resolve_secret_input(username, password, apikey)?;
                    let path = add_secret(&secret_id, username, password, apikey, &keystore_password)?;
                    tracing::info!("Secret '{secret_id}' saved to {path}");
                    Ok(CommandResult::outcome(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::Added,
                        secret_id: Some(secret_id),
                        path: Some(path),
                    }))
                }
                KeystoreCommands::Update {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let (username, password, apikey) = resolve_secret_input(username, password, apikey)?;
                    let path = update_secret(&secret_id, username, password, apikey, &keystore_password)?;
                    tracing::info!("Secret '{secret_id}' updated in {path}");
                    Ok(CommandResult::outcome(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::Updated,
                        secret_id: Some(secret_id),
                        path: Some(path),
                    }))
                }
                KeystoreCommands::Remove {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let expected = expected_secret_auth(username, password, apikey)?;
                    let path = remove_secret(&secret_id, expected, &keystore_password)?;
                    tracing::info!("Secret '{secret_id}' deleted from {path}");
                    Ok(CommandResult::outcome(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::Removed,
                        secret_id: Some(secret_id),
                        path: Some(path),
                    }))
                }
                KeystoreCommands::Unlock { ttl } => {
                    let ttl = ttl
                        .as_deref()
                        .map(parse_unlock_ttl)
                        .transpose()?
                        .unwrap_or_else(default_unlock_ttl);
                    let unlock_path = unlock_keystore(ttl)?;
                    let status = get_unlock_status()?;
                    if let Some(expires_at_epoch) = status.expires_at_epoch {
                        tracing::info!(
                            "Keystore unlocked via {} until {} ({})",
                            unlock_path.display(),
                            format_epoch(expires_at_epoch),
                            format_remaining_duration(expires_at_epoch)
                        );
                    } else {
                        tracing::info!("Keystore unlocked via {}", unlock_path.display());
                    }
                    Ok(CommandResult::outcome(CliOutcome::KeystoreStatus {
                        exists: status.keystore_exists,
                        unlock_active: status.unlock_active,
                        expires_at_epoch: status.expires_at_epoch,
                    }))
                }
                KeystoreCommands::Lock => {
                    let status = esdiag::data::UnlockStatus {
                        keystore_exists: keystore_exists()?,
                        unlock_active: false,
                        expires_at_epoch: None,
                        unlock_path: esdiag::data::get_unlock_path()?,
                    };
                    if clear_unlock_lease()? {
                        tracing::info!("Keystore unlock lease removed");
                    } else {
                        tracing::info!("Keystore unlock lease was already absent");
                    }
                    Ok(CommandResult::outcome(CliOutcome::KeystoreStatus {
                        exists: status.keystore_exists,
                        unlock_active: false,
                        expires_at_epoch: None,
                    }))
                }
                KeystoreCommands::Status => {
                    let status = get_unlock_status()?;
                    let keystore_path = get_keystore_path()?;
                    tracing::info!(
                        "Keystore: {} ({})",
                        if status.keystore_exists { "present" } else { "absent" },
                        keystore_path.display()
                    );
                    Ok(CommandResult::outcome(CliOutcome::KeystoreStatus {
                        exists: status.keystore_exists,
                        unlock_active: status.unlock_active,
                        expires_at_epoch: status.expires_at_epoch,
                    }))
                }
                KeystoreCommands::Password => {
                    if !keystore_exists()? {
                        return Err(eyre!("No keystore exists to update the password."));
                    }
                    let current_password = get_password_for_secret_commands()?;
                    validate_existing_keystore_password(&current_password)?;
                    let new_password = prompt_new_keystore_password()?;
                    let path = rotate_keystore_password(&current_password, &new_password)?;
                    tracing::info!("Keystore password updated for {path}");
                    Ok(CommandResult::outcome(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::PasswordChanged,
                        secret_id: None,
                        path: Some(path),
                    }))
                }
                KeystoreCommands::Migrate => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let (migrated, unchanged) = KnownHost::migrate_hosts_to_keystore(&keystore_password)?;
                    tracing::info!(
                        "Keystore migration complete: migrated {migrated} host(s), unchanged {unchanged} host(s)."
                    );
                    Ok(CommandResult::outcome(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::Migrated,
                        secret_id: None,
                        path: Some(format!("migrated={migrated},unchanged={unchanged}")),
                    }))
                }
            },
            Commands::Process {
                input,
                output,
                #[cfg(feature = "agent")]
                ask,
                account,
                case,
                opportunity,
                user,
                sources,
                #[cfg(feature = "keystore")]
                save_job,
            } => {
                let has_explicit_output = output.is_some();
                #[cfg(feature = "keystore")]
                if let Some(name) = save_job.as_deref() {
                    let identifiers =
                        Identifiers::new(account.clone(), case.clone(), None, opportunity.clone(), user.clone());
                    let job = derive_process_job(&input, output.as_deref(), identifiers)?;
                    esdiag::job::save_job(name, job)?;
                }
                let input_uri = Uri::try_from(input)?;
                let output_uri = Uri::try_from(output.clone())?;
                let stdout_owned = matches!(output_uri, Uri::Stream);
                if has_explicit_output {
                    ensure_uri_role(&output_uri, HostRole::Send, "process output")?;
                }
                #[cfg(feature = "agent")]
                if ask.is_some() && stdout_owned {
                    return Err(eyre!(ProcessAskStdoutConflict));
                }
                #[cfg(feature = "agent")]
                if ask.is_some() {
                    OutputDeployment::resolve(output.as_deref(), true)?;
                }

                tracing::info!("input: {}", input_uri);

                let receiver = Receiver::try_from(input_uri.clone())?;
                if let Some(sources) = sources {
                    let application = detect_sources_application_for_process(&input_uri, &receiver).await?;
                    esdiag::processor::init_sources(sources_application_key(application)?, sources)?;
                }
                let identifiers = Identifiers::new(account, case, receiver.filename(), opportunity, user);
                let mut context = esdiag::job::context::ExecutionContext::default();
                let job_input = match &input_uri {
                    Uri::File(_) | Uri::Directory(_) => esdiag::job::model::Input::Load { uri: input_uri.clone() },
                    _ => {
                        let binding = esdiag::job::model::BindingKey::try_new("cli-process-input")?;
                        let application = match &input_uri {
                            Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
                                host.app()
                            }
                            _ => None,
                        };
                        context.inputs.bind_receiver(binding.clone(), receiver, application);
                        esdiag::job::model::Input::LoadBinding { binding }
                    }
                };
                let export_target = match (&output, &output_uri) {
                    (Some(name), Uri::KnownHost(_)) => {
                        esdiag::job::model::ExportTarget::KnownHost { name: name.clone() }
                    }
                    (_, Uri::File(path)) => esdiag::job::model::ExportTarget::File { path: path.clone() },
                    (_, Uri::Directory(output_dir)) => esdiag::job::model::ExportTarget::Directory {
                        output_dir: output_dir.clone(),
                    },
                    (_, Uri::Stream) => esdiag::job::model::ExportTarget::Stdout,
                    _ => {
                        let binding = esdiag::job::model::BindingKey::try_new("cli-process-output")?;
                        context.bind_document_exporter(
                            binding.clone(),
                            esdiag::exporter::DocumentExporter::try_from(output_uri)?,
                        );
                        esdiag::job::model::ExportTarget::Binding { binding }
                    }
                };
                let job = esdiag::job::model::Job::try_new(
                    identifiers,
                    job_input,
                    None,
                    Some(esdiag::job::model::Process {
                        selection: None,
                        export: export_target,
                    }),
                    None,
                )?;
                let outcome = esdiag::job::executor::execute_with_context(job, context).await;
                let process = execution_process_result(&outcome);
                if !outcome.succeeded() || outcome.diagnostic_outcome() == Some(DiagnosticOutcome::Failed) {
                    return Err(eyre!("{}", format_execution_failure(&outcome)));
                }
                if stdout_owned {
                    Ok(CommandResult::stream())
                } else {
                    let diagnostic = process
                        .map(|result| result.diagnostic)
                        .ok_or_else(|| eyre!("Process completed without a diagnostic report"))?;
                    #[cfg(feature = "agent")]
                    if let Some(prompt) = ask {
                        return run_agent_ask_for_output(
                            process_ask_prompt(&diagnostic.id, &prompt),
                            DEFAULT_AGENT_BUILDER_AGENT.to_string(),
                            None,
                            output.as_deref(),
                        )
                        .await;
                    }
                    Ok(CommandResult::outcome(CliOutcome::DiagnosticProcessed { diagnostic }))
                }
            }
            Commands::Upload {
                file_name,
                upload_id,
                api_url,
            } => {
                let file_path = uploader::default_upload_path(&file_name);
                tracing::info!(
                    "Uploading raw diagnostic archive {} to {}",
                    file_path.display(),
                    upload_id
                );
                let response = uploader::upload_file(&file_path, &upload_id, &api_url).await?;
                tracing::info!("Upload complete for slug {}", response.slug);
                Ok(CommandResult::outcome(CliOutcome::DiagnosticUploaded {
                    destination: format!("{}/g/{}", api_url.trim_end_matches('/'), response.slug),
                }))
            }
            #[cfg(feature = "setup")]
            Commands::Setup { host } => {
                if let Some(host) = host {
                    let uri = Uri::try_from(host)?;
                    let client = Client::try_from(uri)?;
                    tracing::info!("Setting up assets in {client}");
                    setup::assets(&client).await?;
                    Ok(CommandResult::outcome(CliOutcome::SetupCompleted {
                        targets: vec![client.to_string()],
                    }))
                } else {
                    tracing::debug!("Setting up assets with the resolved output deployment");
                    let deployment = OutputDeployment::resolve(None, true)?;
                    let es_uri = Uri::try_from(deployment.elasticsearch)?;
                    let es_client = Client::try_from(es_uri)?;
                    tracing::info!("Setting up assets in {es_client}");
                    setup::assets(&es_client).await?;
                    setup::ensure_agent_builder_license(&es_client).await?;
                    let kibana = deployment
                        .kibana
                        .ok_or_else(|| eyre!("Resolved output deployment is missing a Kibana viewer"))?;
                    let kb_uri = Uri::try_from(kibana)?;
                    let kb_client = Client::try_from(kb_uri)?;
                    tracing::info!("Setting up Kibana assets in {kb_client}");
                    setup::assets(&kb_client).await?;
                    Ok(CommandResult::outcome(CliOutcome::SetupCompleted {
                        targets: vec![es_client.to_string(), kb_client.to_string()],
                    }))
                }
            }
            #[cfg(feature = "keystore")]
            Commands::Job { command } => match command {
                JobCommands::List => {
                    let jobs = esdiag::data::load_saved_jobs()?
                        .into_iter()
                        .map(|(name, job)| saved_job_result(name, &job))
                        .collect();
                    Ok(CommandResult::outcome(CliOutcome::JobsListed { jobs }))
                }
                JobCommands::Run { name } => {
                    let stdout_owned = esdiag::data::load_saved_jobs()?
                        .get(&name)
                        .and_then(|job| job.process())
                        .is_some_and(|process| matches!(process.export, esdiag::job::model::ExportTarget::Stdout));
                    let outcome = esdiag::job::handle_job_run(&name).await?;
                    if stdout_owned {
                        Ok(CommandResult::stream())
                    } else {
                        Ok(CommandResult::outcome(job_outcome(name, outcome)))
                    }
                }
                JobCommands::Delete { name } => {
                    esdiag::job::handle_job_delete(&name)?;
                    Ok(CommandResult::outcome(CliOutcome::JobDeleted { name }))
                }
            },
        }
    } else {
        #[cfg(all(feature = "server", feature = "desktop"))]
        {
            // Set up communication channel to tell the server when to shut down
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

            // Tauri desktop wrapper logic
            tauri::Builder::default()
                .plugin(tauri_plugin_opener::init())
                .setup(|app| {
                    use tauri::Manager;

                    let handle = app.handle().clone();

                    tauri::async_runtime::spawn(async move {
                        let settings = esdiag::data::Settings::load().unwrap_or_default();

                        let exporter = if let Some(target) = &settings.active_target {
                            if let Ok(host) =
                                esdiag::data::KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found"))
                            {
                                if let Ok(uri) = Uri::try_from(host) {
                                    Exporter::try_from(uri).unwrap_or_default()
                                } else {
                                    Exporter::default()
                                }
                            } else {
                                Exporter::default()
                            }
                        } else {
                            Exporter::default()
                        };

                        let kibana_url = settings.kibana_url.unwrap_or_else(|| {
                            let url = esdiag::env::get_string("ESDIAG_KIBANA_URL")
                                .unwrap_or_else(|_| "http://localhost:5601".to_string());
                            esdiag::env::append_kibana_space(&url)
                        });

                        let (mut server, bound_addr) =
                            match Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User).await {
                                Ok(res) => res,
                                Err(e) => {
                                    tracing::error!("Failed to start embedded server: {}", e);
                                    return;
                                }
                            };

                        let url = format!("http://localhost:{}", bound_addr.port());

                        if let Ok(url) = tauri::Url::parse(&url) {
                            // Wait a tiny bit to ensure server is ready
                            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                            if let Some(window) = handle.get_webview_window("main") {
                                window.on_window_event(|event| {
                                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                                        // Custom close logic if needed
                                    }
                                });
                                let _ = window.navigate(url);
                                let _ = window.set_focus();
                            }
                        }

                        // Wait for Tauri exit signal
                        let _ = shutdown_rx.recv().await;
                        server.shutdown().await;
                    });

                    Ok(())
                })
                .on_window_event(move |_window, event| {
                    use tauri::WindowEvent;
                    if let WindowEvent::Destroyed = event {
                        // All windows closed, signal the server to shut down
                        let _ = shutdown_tx.try_send(());
                    }
                })
                .run(tauri::generate_context!())
                .expect("error while running tauri application");

            Ok(CommandResult::stream())
        }
        #[cfg(not(all(feature = "server", feature = "desktop")))]
        {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            cmd.print_help()?;
            Err(eyre!(
                "No command provided. If you want to use the Desktop UI, compile with the 'desktop' feature."
            ))
        }
    }
}

#[cfg(feature = "keystore")]
fn derive_collect_job(
    host: &str,
    output: &str,
    diagnostic_type: &str,
    upload_id: Option<&str>,
    identifiers: Identifiers,
) -> Result<esdiag::data::Job> {
    let builder = esdiag::data::Job::builder()
        .identifiers(identifiers)
        .collect_from(host)?
        .diagnostic_type(diagnostic_type);

    match upload_id {
        Some(upload_id) => builder.upload_to(upload_id),
        None => builder.collect_to(output),
    }
}

#[cfg(feature = "keystore")]
fn derive_process_job(input: &str, output: Option<&str>, identifiers: Identifiers) -> Result<esdiag::data::Job> {
    let output = output.ok_or_else(|| eyre!("Saved jobs require an explicit process output target"))?;
    let output_uri = Uri::try_from(output.to_string())?;
    ensure_uri_role(&output_uri, HostRole::Send, "save-job output")?;
    let output = esdiag::data::JobOutput::from_cli_target(output)?;
    esdiag::data::Job::builder()
        .identifiers(identifiers)
        .collect_from(input)?
        .process_to(output)
}

fn sources_application_key(application: Application) -> Result<&'static str> {
    esdiag::processor::diagnostic::data_source::source_application_key(application).map_err(|_| {
        eyre!(
            "--sources is only supported for Elasticsearch, Kibana, and Logstash inputs, got {}",
            application
        )
    })
}

async fn detect_sources_application_for_process(input_uri: &Uri, receiver: &Receiver) -> Result<Application> {
    match input_uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => host
            .app()
            .ok_or_else(|| eyre!("--sources is only supported for application diagnostics")),
        _ => receiver
            .try_get_manifest_from_files()
            .await?
            .application()
            .ok_or_else(|| eyre!("--sources is only supported for application diagnostics")),
    }
}

#[cfg(all(feature = "server", unix))]
async fn wait_for_shutdown_signal() -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Shutting down server (Ctrl+C)...");
        }
        _ = async {
            let mut term_signal = signal(SignalKind::terminate())
                .map_err(|e| eyre!("Failed to install SIGTERM handler: {}", e))?;
            term_signal.recv().await;
            tracing::info!("Shutting down server (SIGTERM)...");
            Ok::<_, eyre::Report>(())
        } => {}
    }

    Ok(())
}

#[cfg(all(feature = "server", not(unix)))]
async fn wait_for_shutdown_signal() -> Result<()> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| eyre!("Failed to install Ctrl+C handler: {}", e))?;
    tracing::info!("Shutting down server (Ctrl+C)...");
    Ok(())
}

fn expected_secret_auth(
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
) -> Result<Option<SecretAuth>> {
    match (apikey, username, password) {
        (None, None, None) => Ok(None),
        (Some(apikey), None, None) => Ok(Some(SecretAuth::apikey(apikey))),
        (None, Some(username), Some(password)) => Ok(Some(SecretAuth::basic(username, password))),
        _ => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
    }
}

fn build_host_cli_update(args: HostMutationArgs) -> KnownHostCliUpdate {
    args.into()
}

fn infer_product_from_url(url: &Url) -> Option<Application> {
    match url.port_or_known_default() {
        Some(9200) => Some(Application::Elasticsearch),
        Some(5601) => Some(Application::Kibana),
        Some(9600) => Some(Application::Logstash),
        _ => {
            let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
            if matches!(segments.next(), Some("api"))
                && matches!(segments.next(), Some("v1"))
                && matches!(segments.next(), Some("deployments"))
            {
                let _deployment_id = segments.next()?;
                return Application::from_str(segments.next()?).ok();
            }
            None
        }
    }
}

fn build_host_from_definition(
    app: Option<Application>,
    target: &str,
    use_url_template: bool,
    update: &KnownHostCliUpdate,
    secret_auth: Option<SecretAuth>,
) -> Result<KnownHost> {
    let mut builder = if use_url_template {
        KnownHostBuilder::new_template(target.to_string())
    } else {
        let url = Url::parse(target).map_err(|err| eyre!("Invalid host target URL: {err}"))?;
        let inferred_app = infer_product_from_url(&url);
        let app = app
            .or(inferred_app)
            .ok_or_else(|| eyre!("Target '{target}' does not determine the app. Rerun with `--app <app>`."))?;
        KnownHostBuilder::new(url).application(app)
    }
    .accept_invalid_certs(update.accept_invalid_certs.unwrap_or(false))
    .legacy_credentials(update.apikey.clone(), update.username.clone(), update.password.clone())
    .secret(update.secret.clone());
    if let Some(app) = app {
        builder = builder.application(app);
    }
    if let Some(roles) = update.roles.clone() {
        builder = builder.roles(roles);
    }
    match secret_auth {
        Some(secret_auth) => builder.build_with_secret_auth(secret_auth),
        None => builder.build(),
    }
}

fn maybe_materialize_template_target(target: &str) -> Result<Option<KnownHost>> {
    KnownHost::resolve_template_reference(target)
}

async fn save_host(name: &str, host: KnownHost, action: &str, validate_connection: bool) -> Result<String> {
    if validate_connection {
        let uri = Uri::try_from(host.clone())?;
        let validation_summary = validate_host_connection(name, uri).await?;
        let hostfile = host.save(name)?;
        tracing::info!("Host {name} successfully saved to {hostfile}");
        return Ok(format!("{validation_summary}\nHost '{name}' {action} in {hostfile}"));
    }
    let hostfile = host.save(name)?;
    tracing::info!("Host {name} successfully saved to {hostfile}");
    Ok(format!("Host '{name}' {action} in {hostfile}"))
}

fn legacy_host_command_error(args: &[String]) -> eyre::Report {
    let attempted = if args.is_empty() {
        "esdiag host".to_string()
    } else {
        format!("esdiag host {}", args.join(" "))
    };
    eyre!(
        "Legacy positional host syntax is no longer supported. Use `esdiag host add <name> <target> [--app <app>]` to create a host, `esdiag host update <name>` to modify one, `esdiag host remove <name>` to delete one, `esdiag host list` to inspect saved hosts, or `esdiag host auth <target>` to test a saved host or resolved template reference. Received: `{attempted}`"
    )
}

fn cleanup_settings_after_host_delete(name: &str) -> Result<()> {
    let mut settings = Settings::load()?;
    if settings.active_target.as_deref() != Some(name) {
        return Ok(());
    }

    let hosts = KnownHost::parse_hosts_yml()?;
    settings.active_target = hosts.keys().next().cloned();
    settings.save()
}

fn delete_host_from_cli(name: &str) -> Result<String> {
    let path = KnownHost::remove_saved(name)?;
    if let Err(err) = cleanup_settings_after_host_delete(name) {
        eprintln!(
            "Warning: host '{}' was removed, but failed to update settings: {err}",
            name
        );
    }
    Ok(path)
}
fn resolve_secret_input(
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    resolve_secret_input_with_prompt(username, password, apikey, prompt_missing_secret_value)
}

fn resolve_secret_input_with_prompt<F>(
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
    mut prompt_secret: F,
) -> Result<(Option<String>, Option<String>, Option<String>)>
where
    F: FnMut(&str) -> Result<String>,
{
    let requested_apikey_prompt = apikey.as_ref().is_some_and(|value| value.trim().is_empty());
    let requested_password_prompt = password.as_ref().is_some_and(|value| value.trim().is_empty());
    let username = normalize_optional_secret_arg(username);
    let mut password = normalize_optional_secret_arg(password);
    let mut apikey = normalize_optional_secret_arg(apikey);
    if requested_apikey_prompt {
        apikey = Some(prompt_secret("Enter secret API key: ")?);
    }
    match (&apikey, &username, &password) {
        (Some(_), None, None) => Ok((None, None, apikey)),
        (None, Some(_), Some(_)) => Ok((username, password, None)),
        (None, Some(_), None) => {
            if requested_password_prompt || password.is_none() {
                password = Some(prompt_secret("Enter secret password: ")?);
            }
            Ok((username, password, None))
        }
        (None, None, Some(_)) => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
        (None, None, None) => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
        (Some(_), _, _) => Ok((None, None, apikey)),
    }
}

fn normalize_optional_secret_arg(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputLocation {
    Local,
    Remote,
}

fn prompt_onboarding_workflow() -> Result<OnboardingWorkflow> {
    loop {
        println!("Will you be processing diagnostics, or only collecting?");
        println!("  1. Process diagnostics");
        println!("  2. Only collect diagnostics");
        match prompt_with_default("Selection", "1")?.to_ascii_lowercase().as_str() {
            "1" | "process" | "processing" => {
                println!("Will you collect new diagnostics, process existing diagnostics, or both?");
                println!("  1. Collect and process new diagnostics");
                println!("  2. Process existing diagnostics");
                println!("  3. Both new and existing diagnostics");
                match prompt_with_default("Selection", "3")?.to_ascii_lowercase().as_str() {
                    "1" | "collect" => return Ok(OnboardingWorkflow::CollectAndProcess),
                    "2" | "existing" => return Ok(OnboardingWorkflow::ProcessExisting),
                    "3" | "both" => return Ok(OnboardingWorkflow::CollectAndProcess),
                    _ => println!("Choose 1, 2, or 3."),
                }
            }
            "2" | "collect" | "collect-only" => return Ok(OnboardingWorkflow::CollectOnly),
            _ => println!("Choose 1 or 2."),
        }
    }
}

fn prompt_output_location() -> Result<OutputLocation> {
    loop {
        println!("Will processed diagnostics be stored locally or remotely?");
        println!("  1. Local ESDiag deployment");
        println!("  2. Remote Elasticsearch and Kibana deployment");
        match prompt_with_default("Selection", "2")?.to_ascii_lowercase().as_str() {
            "1" | "local" => return Ok(OutputLocation::Local),
            "2" | "remote" => return Ok(OutputLocation::Remote),
            _ => println!("Choose 1 or 2."),
        }
    }
}

fn prompt_missing_secret_value(prompt: &str) -> Result<String> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!(
            "Required secret value was not provided and no interactive terminal is available."
        ));
    }
    let value = rpassword::prompt_password(prompt)?;
    if value.is_empty() {
        return Err(eyre!("Required secret value was not provided."));
    }
    Ok(value)
}

fn prompt_confirm(message: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Ok(false);
    }
    print!("{message}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

fn prompt_confirm_default_yes(message: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Ok(false);
    }
    print!("{message}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(!matches!(answer.as_str(), "n" | "no"))
}

fn prompt_new_keystore_password() -> Result<String> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!("A new keystore password requires an interactive terminal."));
    }
    let password = rpassword::prompt_password("Enter new keystore password: ")?;
    if password.is_empty() {
        return Err(eyre!("Keystore password cannot be empty."));
    }
    let confirm = rpassword::prompt_password("Confirm new keystore password: ")?;
    if password != confirm {
        return Err(eyre!("Keystore password confirmation did not match."));
    }
    Ok(password)
}

fn unlock_keystore(ttl: Duration) -> Result<std::path::PathBuf> {
    if keystore_exists()? {
        let keystore_password = get_password_for_secret_commands()?;
        validate_existing_keystore_password(&keystore_password)?;
        return write_unlock_lease(&keystore_password, ttl);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!(
            "No keystore exists and no interactive terminal is available to create one."
        ));
    }
    if !prompt_confirm("No keystore exists. Create one now? [y/N]: ")? {
        return Err(eyre!("Keystore unlock cancelled."));
    }
    let keystore_password = prompt_new_keystore_password()?;
    create_keystore(&keystore_password)?;
    write_unlock_lease(&keystore_password, ttl)
}

async fn run_init_wizard() -> Result<CommandResult> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!("`esdiag init` requires an interactive controlling terminal."));
    }

    let initial = inspect_onboarding()?;
    println!("ESDiag first-run initialization");
    let mut output_name_for_defaults = esdiag::data::ApplicationConfig::load()?.output.default;
    let mut output_url_for_defaults = output_name_for_defaults
        .as_ref()
        .and_then(KnownHost::get_known)
        .and_then(|host| host.concrete_url().map(Url::to_string));
    let mut most_recent_collect_host = None;
    let mut started_local_stack = false;
    if initial.is_complete() && !prompt_confirm("A complete configuration already exists. Replace values? [y/N]: ")? {
        return Ok(CommandResult::outcome(initialization_outcome(
            initialization_skill_installation()?,
        )?));
    }

    let config = esdiag::data::ApplicationConfig::load()?;
    let replace_user = match config.user.as_deref() {
        Some(user) if !prompt_confirm_default_yes(&format!("Resume configuring user: {user} [Y/n]: "))? => {
            prompt_confirm("Replace existing configuration? [y/N]: ")?
        }
        Some(_) => false,
        None => true,
    };
    if replace_user {
        save_user(prompt_with_default(
            "Default diagnostic user",
            &default_diagnostic_user(),
        )?)?;
    }

    let workflow = match config.workflow {
        Some(workflow) if prompt_confirm_default_yes(&format!("Resume workflow: {} [Y/n]: ", workflow.as_str()))? => {
            workflow
        }
        _ => prompt_onboarding_workflow()?,
    };
    if config.workflow != Some(workflow) {
        save_workflow(workflow)?;
    }
    if workflow == OnboardingWorkflow::ProcessExisting
        && let Some(job) = esdiag::data::ApplicationConfig::load()?.job.default
    {
        if !prompt_confirm(&format!(
            "Processing existing diagnostics does not use a default job. Clear '{job}' as the default? [y/N]: "
        ))? {
            return Err(eyre!("Default job removal was declined."));
        }
        let mut config = esdiag::data::ApplicationConfig::load()?;
        config.job.default = None;
        config.save()?;
    }

    if workflow.processes_diagnostics() {
        let reuse_output = initial.output_configured
            && config.workflow == Some(workflow)
            && prompt_confirm_default_yes(&format!(
                "Resume configured output deployment: {} [Y/n]: ",
                output_name_for_defaults.as_deref().unwrap_or("diagnostics")
            ))?;
        if !reuse_output {
            unlock_keystore(default_unlock_ttl())?;
            let keystore_password = get_password_for_secret_commands()?;
            let (output_name, output_url, viewer_url, viewer_api_url, secret_id, auth) = match prompt_output_location()?
            {
                OutputLocation::Local => {
                    let local_output = |preset: EsdiagLocalPreset| -> Result<_> {
                        let apikey = preset.apikey.ok_or_else(|| {
                        eyre!(
                            "The local ESDiag deployment has no usable output API key. Run `esdiag local up` first, or select remote processing."
                        )
                    })?;
                        Ok((
                            "localhost".to_string(),
                            Url::parse(&preset.elasticsearch_url)?,
                            Url::parse(&preset.kibana_url)?,
                            Url::parse(&preset.kibana_api_url)?,
                            "localhost".to_string(),
                            SecretAuth::apikey(apikey),
                        ))
                    };
                    let existing = detected_esdiag_local_preset();
                    let runtime_available = if existing.is_none() {
                        match local::detected_runtime() {
                            Some(runtime) => {
                                println!("Detected container runtime: {runtime}");
                                true
                            }
                            None => {
                                println!("No container runtime detected; cannot configure a local stack.");
                                false
                            }
                        }
                    } else {
                        true
                    };
                    let start_core = should_start_local_core_stack(existing.is_none(), runtime_available, || {
                        prompt_confirm("No local ESDiag deployment was detected. Start a local core stack now? [y/N]: ")
                    })?;
                    let local_preset = match existing {
                        Some(preset) => Some(preset),
                        None if start_core => {
                            run_local_lifecycle(local_core_stack_start_args()).await?;
                            started_local_stack = true;
                            detected_esdiag_local_preset()
                        }
                        None => None,
                    };
                    match local_preset {
                        Some(preset) => local_output(preset)?,
                        None => {
                            if !prompt_confirm_default_yes("Configure a remote output deployment instead? [Y/n]: ")? {
                                return Err(eyre!(
                                    "Local stack startup was declined. Select local output again to start it, or configure a remote output."
                                ));
                            }
                            let output_name = prompt_with_default("Remote output host name", "diagnostics")?;
                            let output_url =
                                prompt_url_with_default("Remote output Elasticsearch URL", "https://localhost:9200")?;
                            let viewer_url =
                                prompt_url_with_default("Remote output Kibana URL", "https://localhost:5601")?;
                            let secret_id = prompt_with_default("Remote output credential name", &output_name)?;
                            let auth = prompt_api_key("Remote output", None)?;
                            (output_name, output_url, viewer_url.clone(), viewer_url, secret_id, auth)
                        }
                    }
                }
                OutputLocation::Remote => {
                    let output_name = prompt_with_default("Remote output host name", "diagnostics")?;
                    let output_url =
                        prompt_url_with_default("Remote output Elasticsearch URL", "https://localhost:9200")?;
                    let viewer_url = prompt_url_with_default("Remote output Kibana URL", "https://localhost:5601")?;
                    let secret_id = prompt_with_default("Remote output credential name", &output_name)?;
                    let auth = prompt_api_key("Remote output", None)?;
                    (output_name, output_url, viewer_url.clone(), viewer_url, secret_id, auth)
                }
            };
            let viewer_name = format!("{output_name}-kb");
            let output_candidate = KnownHostBuilder::new(output_url.clone())
                .application(Application::Elasticsearch)
                .roles(vec![HostRole::Send])
                .viewer(Some(viewer_name.clone()))
                .secret(Some(secret_id.clone()))
                .build_with_secret_auth(auth.clone())?;
            let viewer_candidate = KnownHostBuilder::new(viewer_api_url)
                .application(Application::Kibana)
                .roles(vec![HostRole::View])
                .secret(Some(secret_id.clone()))
                .build_with_secret_auth(auth.clone())?;
            let output_client = Client::try_from(Uri::try_from(output_candidate)?)?;
            let output_valid = output_client.test_connection().await.is_ok();
            let viewer_client = Client::try_from(Uri::try_from(viewer_candidate)?)?;
            let viewer_valid = viewer_client.test_connection().await.is_ok();
            ensure_output_deployment_valid(output_valid, viewer_valid)?;
            confirm_output_replacement(&output_name, &viewer_name, &secret_id, &keystore_password)?;
            output_name_for_defaults = Some(output_name.clone());
            output_url_for_defaults = Some(output_url.to_string());
            save_output_deployment(
                OutputDeploymentInput {
                    output_name,
                    output_url,
                    viewer_name,
                    viewer_url,
                    secret_id,
                    auth,
                },
                &keystore_password,
            )?;

            let mut output_config = esdiag::data::ApplicationConfig::load()?;
            output_config.output.authenticated_on = Some(chrono::Utc::now().to_rfc3339());
            #[cfg(feature = "setup")]
            let run_output_setup = should_run_output_setup(started_local_stack, || {
                prompt_confirm("Does the diagnostic cluster need ESDiag's dashboards and agents installed? [y/N]: ")
            })?;
            #[cfg(feature = "setup")]
            if started_local_stack {
                output_config.output.assets_version = Some(env!("CARGO_PKG_VERSION").to_string());
            } else if run_output_setup {
                setup::assets(&output_client).await?;
                setup::ensure_agent_builder_license(&output_client).await?;
                setup::assets(&viewer_client).await?;
                output_config.output.assets_version = Some(env!("CARGO_PKG_VERSION").to_string());
            } else {
                eprintln!(
                    "ESDiag assets were not installed. Processing and Agent Builder are unavailable until you run `esdiag setup {}`.",
                    output_config.output.default.as_deref().unwrap_or("diagnostics")
                );
            }
            output_config.save()?;
        }
    }

    if workflow.collects_diagnostics() {
        let existing_collect_host = default_collect_host_name();
        if let Some(name) = existing_collect_host
            && prompt_confirm_default_yes(&format!("Resume collection host: {name} [Y/n]: "))?
        {
            most_recent_collect_host = Some(name);
        } else {
            let collect_name_default = output_name_for_defaults.clone().unwrap_or_else(|| "source".to_string());
            let name = prompt_with_default("Collection host name", &collect_name_default)?;
            let url = prompt_url_with_default(
                "Collection Elasticsearch URL",
                output_url_for_defaults.as_deref().unwrap_or("http://localhost:9200"),
            )?;
            let reuse_output_secret = output_name_for_defaults.is_some()
                && prompt_confirm_default_yes("Reuse the processing credential for this host? [Y/n]: ")?;
            let secret_id = if reuse_output_secret {
                esdiag::data::ApplicationConfig::load()?
                    .output
                    .default
                    .and_then(|output| KnownHost::get_known(&output))
                    .and_then(|host| host.secret)
            } else {
                Some(prompt_with_default("Collection credential name", &name)?)
            };
            let auth = if reuse_output_secret {
                None
            } else {
                Some(prompt_api_key("Collection", detected_esdiag_local_preset().as_ref())?)
            };
            let keystore_password = if auth.is_some() {
                unlock_keystore(default_unlock_ttl())?;
                Some(get_password_for_secret_commands()?)
            } else {
                None
            };
            if KnownHost::get_known(&name).is_some()
                && !prompt_confirm(&format!("Add the collect role to existing host '{name}'? [y/N]: "))?
            {
                return Err(eyre!("Collection host replacement was declined."));
            }
            save_collect_host(
                CollectHostInput {
                    name: name.clone(),
                    app: Application::Elasticsearch,
                    url,
                    secret_id,
                    auth,
                },
                keystore_password.as_deref(),
            )?;
            most_recent_collect_host = Some(name);
        }
    }

    if workflow.collects_diagnostics() {
        let collect_host_default =
            most_recent_collect_host.ok_or_else(|| eyre!("Add a collect host before creating the default job"))?;
        let collect_host = prompt_with_default("Collect host for the default job", &collect_host_default)?;
        let collect_job_name = format!("{collect_host}-collect");
        let collect_job = esdiag::data::Job::builder()
            .collect_from(collect_host.clone())?
            .collect_to(format!("diagnostics/{collect_host}"))?;
        confirm_default_job_replacement(&collect_job_name)?;
        save_default_job(collect_job_name, collect_job)?;
        if workflow.processes_diagnostics() {
            let output = esdiag::data::ApplicationConfig::load()?
                .output
                .default
                .ok_or_else(|| eyre!("A processing workflow requires an output deployment"))?;
            let process_job_name = format!("{collect_host}-process-{output}");
            confirm_default_job_replacement(&process_job_name)?;
            save_default_processing_job(process_job_name, collect_host)?;
        }
    }

    let readiness = inspect_onboarding()?;
    if !readiness.is_complete() {
        return Err(eyre!("Initialization did not produce a complete reusable workflow."));
    }
    let outcome = initialization_outcome(initialization_skill_installation()?)?;
    if started_local_stack {
        run_local_lifecycle(vec![OsString::from("open")]).await?;
    }
    Ok(CommandResult::outcome(outcome))
}

fn should_start_local_core_stack(
    no_existing_stack: bool,
    runtime_available: bool,
    approved: impl FnOnce() -> Result<bool>,
) -> Result<bool> {
    if no_existing_stack && runtime_available {
        approved()
    } else {
        Ok(false)
    }
}

fn ensure_output_deployment_valid(elasticsearch_valid: bool, kibana_valid: bool) -> Result<()> {
    match (elasticsearch_valid, kibana_valid) {
        (true, true) => Ok(()),
        (false, false) => Err(eyre!(
            "The selected Elasticsearch and Kibana output endpoints could not be validated. Existing output configuration was not changed."
        )),
        (false, true) => Err(eyre!(
            "The selected Elasticsearch output endpoint could not be validated. Existing output configuration was not changed."
        )),
        (true, false) => Err(eyre!(
            "The selected Kibana output endpoint could not be validated. Existing output configuration was not changed."
        )),
    }
}

fn confirm_output_replacement(
    output_name: &str,
    viewer_name: &str,
    secret_id: &str,
    keystore_password: &str,
) -> Result<()> {
    let hosts = KnownHost::parse_hosts_yml()?;
    let replaces_output = hosts.contains_key(output_name);
    let replaces_viewer = hosts.contains_key(viewer_name);
    let replaces_secret = list_secret_names(keystore_password)?
        .iter()
        .any(|name| name == secret_id);
    let replaces_default = esdiag::data::ApplicationConfig::load()?
        .output
        .default
        .is_some_and(|name| name != output_name);
    if replaces_output || replaces_viewer || replaces_secret || replaces_default {
        let mut replaced = Vec::new();
        if replaces_output {
            replaced.push(format!("output host '{output_name}'"));
        }
        if replaces_viewer {
            replaced.push(format!("Kibana viewer '{viewer_name}'"));
        }
        if replaces_secret {
            replaced.push(format!("secret '{secret_id}'"));
        }
        if replaces_default {
            replaced.push("the configured default output".to_string());
        }
        if !prompt_confirm(&format!("Replace {}? [y/N]: ", replaced.join(", ")))? {
            return Err(eyre!("Output deployment replacement was declined."));
        }
    }
    Ok(())
}

fn default_collect_host_name() -> Option<String> {
    KnownHost::parse_hosts_yml()
        .ok()?
        .into_iter()
        .find_map(|(name, host)| host.has_role(HostRole::Collect).then_some(name))
}

fn confirm_default_job_replacement(name: &str) -> Result<()> {
    let jobs = esdiag::data::load_saved_jobs()?;
    let config = esdiag::data::ApplicationConfig::load()?;
    let replaces_job = jobs.contains_key(name);
    let replaces_default = config.job.default.as_deref().is_some_and(|current| current != name);
    if (replaces_job || replaces_default)
        && !prompt_confirm(&format!(
            "Replace {}? [y/N]: ",
            match (replaces_job, replaces_default) {
                (true, true) => format!("existing job '{name}' and configured default job"),
                (true, false) => format!("existing job '{name}'"),
                (false, true) => "the configured default job".to_string(),
                (false, false) => unreachable!(),
            }
        ))?
    {
        return Err(eyre!("Default job replacement was declined."));
    }
    Ok(())
}

fn should_run_output_setup(started_local_stack: bool, approved: impl FnOnce() -> Result<bool>) -> Result<bool> {
    if started_local_stack { Ok(false) } else { approved() }
}

fn local_core_stack_start_args() -> Vec<OsString> {
    vec![
        OsString::from("up"),
        OsString::from("--stack=core"),
        OsString::from("--open-browser=false"),
    ]
}

fn initialization_outcome(skill_installation: Option<AgentSkillsResult>) -> Result<CliOutcome> {
    let config = esdiag::data::ApplicationConfig::load()?;
    Ok(CliOutcome::InitializationCompleted {
        user: config
            .user
            .ok_or_else(|| eyre!("Initialization completed without a configured user"))?,
        output: config.output.default,
        job: config.job.default,
        skill_installation,
    })
}

fn initialization_skill_installation() -> Result<Option<AgentSkillsResult>> {
    #[cfg(feature = "agent")]
    {
        run_optional_agent_skill_stage().map(Some)
    }
    #[cfg(not(feature = "agent"))]
    {
        Ok(None)
    }
}

#[cfg(feature = "agent")]
fn run_optional_agent_skill_stage() -> Result<AgentSkillsResult> {
    let environment = SkillEnvironment::current();
    let detected = detected_targets(&environment);
    let recovery_command = "esdiag agent skills [--target <claude|codex|opencode>]...";
    let detected_names = detected.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let prompt = if detected_names.is_empty() {
        "No supported coding-agent home was detected. Install the embedded ESDiag skill now? [y/N]: ".to_string()
    } else {
        format!("Install the embedded ESDiag skill for detected targets ({detected_names})? [y/N]: ")
    };
    if !prompt_confirm(&prompt)? {
        return Ok(AgentSkillsResult {
            selected_targets: vec![],
            results: vec![],
            recovery_command: recovery_command.to_string(),
        });
    }

    let selected = prompt_agent_skill_targets(&detected)?;
    let selected_names = selected.iter().map(ToString::to_string).collect();
    match install_skill_targets(&environment, detected, selected, false) {
        Ok(installation) => {
            if installation.failed {
                eprintln!("Some ESDiag skill targets could not be installed. Recover with `{recovery_command}`.");
            }
            Ok(AgentSkillsResult {
                selected_targets: selected_names,
                results: installation.results,
                recovery_command: recovery_command.to_string(),
            })
        }
        Err(error) => {
            eprintln!("Could not prepare embedded ESDiag skill installation: {error}");
            Ok(AgentSkillsResult {
                selected_targets: selected_names,
                results: vec![],
                recovery_command: recovery_command.to_string(),
            })
        }
    }
}

#[cfg(feature = "agent")]
fn prompt_agent_skill_targets(detected: &[SkillTarget]) -> Result<Vec<SkillTarget>> {
    let defaults = detected.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
    print!(
        "Agent skill targets [{}] (claude,codex,opencode; Enter accepts detected): ",
        if defaults.is_empty() { "none" } else { &defaults }
    );
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let targets = if line.trim().is_empty() {
        detected.to_vec()
    } else {
        line.trim()
            .split(',')
            .map(|target| match target.trim().to_ascii_lowercase().as_str() {
                "claude" => Ok(SkillTarget::Claude),
                "codex" => Ok(SkillTarget::Codex),
                "opencode" => Ok(SkillTarget::OpenCode),
                target => Err(eyre!(
                    "Unknown agent skill target '{target}'. Choose claude, codex, or opencode."
                )),
            })
            .collect::<Result<Vec<_>>>()?
    };
    let mut targets = targets;
    targets.sort_unstable();
    targets.dedup();
    Ok(targets)
}

fn prompt_required(message: &str) -> Result<String> {
    print!("{message}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let value = line.trim().to_string();
    if value.is_empty() {
        return Err(eyre!("A value is required."));
    }
    Ok(value)
}

fn prompt_with_default(message: &str, default: &str) -> Result<String> {
    print!("{message} [{default}]: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let value = line.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn prompt_url_with_default(message: &str, default: &str) -> Result<Url> {
    Url::parse(&prompt_with_default(message, default)?).map_err(Into::into)
}

#[derive(Clone)]
struct EsdiagLocalPreset {
    elasticsearch_url: String,
    kibana_url: String,
    kibana_api_url: String,
    apikey: Option<String>,
}

#[cfg(test)]
fn local_stack_outcome(command: String, args: &[OsString]) -> CliOutcome {
    let state_dir = args
        .iter()
        .enumerate()
        .find_map(|(index, argument)| {
            if argument == "--state-dir" {
                args.get(index + 1).cloned().map(PathBuf::from)
            } else {
                argument
                    .to_str()
                    .and_then(|argument| argument.strip_prefix("--state-dir="))
                    .map(PathBuf::from)
            }
        })
        .or_else(|| std::env::var_os("ESDIAG_LOCAL_DIR").map(PathBuf::from))
        .or_else(default_esdiag_local_state_dir);
    let values = state_dir
        .and_then(|dir| std::fs::read_to_string(dir.join(".env")).ok())
        .map(|contents| {
            contents
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<std::collections::HashMap<_, _>>()
        });
    let mode = values
        .as_ref()
        .and_then(|values| values.get("STACK_MODE"))
        .map(ToString::to_string);
    let esdiag_url = values
        .as_ref()
        .and_then(|values| values.get("ESDIAG_PORT"))
        .map(|port| format!("http://127.0.0.1:{port}"));
    let kibana_url = values
        .as_ref()
        .and_then(|values| values.get("ESDIAG_KIBANA_PORT"))
        .map(|port| format!("http://127.0.0.1:{port}"));
    CliOutcome::LocalStack {
        command,
        mode,
        native_service: None,
        esdiag_url,
        kibana_url,
    }
}

fn detected_esdiag_local_preset() -> Option<EsdiagLocalPreset> {
    if std::env::var("ESDIAG_CONTAINER_LOCAL_STACK").ok().as_deref() == Some("full") {
        let elasticsearch_url = std::env::var("ESDIAG_OUTPUT_URL").ok()?;
        let kibana_api_url = std::env::var("ESDIAG_KIBANA_INTERNAL_URL")
            .or_else(|_| std::env::var("ESDIAG_KIBANA_URL"))
            .ok()?;
        let kibana_url = std::env::var("ESDIAG_KIBANA_PUBLIC_URL").ok()?;
        return Some(EsdiagLocalPreset {
            elasticsearch_url,
            kibana_url,
            kibana_api_url,
            apikey: std::env::var("ESDIAG_OUTPUT_APIKEY")
                .ok()
                .filter(|value| !value.trim().is_empty() && value != "pending"),
        });
    }
    let state_dir = std::env::var_os("ESDIAG_LOCAL_DIR")
        .map(PathBuf::from)
        .or_else(default_esdiag_local_state_dir)?;
    let contents = std::fs::read_to_string(state_dir.join(".env")).ok()?;
    let values = contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            (!line.is_empty() && !line.starts_with('#'))
                .then(|| line.split_once('='))
                .flatten()
        })
        .collect::<std::collections::HashMap<_, _>>();
    if values
        .get("STACK_MODE")
        .is_some_and(|mode| !matches!(mode.trim(), "full" | "core"))
    {
        return None;
    }
    let elasticsearch_port = values.get("ESDIAG_ELASTICSEARCH_PORT").copied().unwrap_or("9200");
    let kibana_port = values.get("ESDIAG_KIBANA_PORT").copied().unwrap_or("5601");
    Some(EsdiagLocalPreset {
        elasticsearch_url: format!("http://localhost:{elasticsearch_port}"),
        kibana_url: format!("http://localhost:{kibana_port}"),
        kibana_api_url: format!("http://localhost:{kibana_port}"),
        apikey: values
            .get("ESDIAG_OUTPUT_APIKEY")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && *value != "pending")
            .map(str::to_string),
    })
}

fn default_esdiag_local_state_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let home = std::env::var_os("USERPROFILE")?;
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".esdiag/local"))
}

fn prompt_api_key(label: &str, local_preset: Option<&EsdiagLocalPreset>) -> Result<SecretAuth> {
    let default = if local_preset.and_then(|preset| preset.apikey.as_ref()).is_some() {
        "3"
    } else {
        "4"
    };
    loop {
        println!("{label} API key source:");
        println!("  1. Read from a file");
        println!("  2. Read from an environment variable");
        println!("  3. Read from detected esdiag-local configuration");
        println!("  4. Paste securely");
        let source = prompt_with_default("Selection", default)?;
        let apikey = match source.as_str() {
            "1" | "file" => prompt_required("API key file path: ").and_then(|path| read_api_key_file(&path)),
            "2" | "environment" | "env" => {
                let name = prompt_with_default("API key environment variable", "ESDIAG_OUTPUT_APIKEY")?;
                std::env::var(&name).map_err(|_| eyre!("{name} is not set"))
            }
            "3" | "esdiag-local" | "local" => local_preset
                .and_then(|preset| preset.apikey.clone())
                .ok_or_else(|| eyre!("No usable ESDiag local API key was detected")),
            "4" | "paste" => prompt_missing_secret_value(&format!("Enter {label} API key: ")),
            _ => Err(eyre!("Choose API key source 1, 2, 3, or 4")),
        };
        let apikey = match apikey {
            Ok(apikey) => apikey.trim().to_string(),
            Err(err) => {
                print_api_key_source_warning(&format!("Unable to read API key source: {err}. Please try again."));
                continue;
            }
        };
        if apikey.is_empty() || apikey == "pending" || apikey.contains('\n') || apikey.contains('\r') {
            print_api_key_source_warning(
                "The selected API key source did not provide one usable API key. Please try again.",
            );
            continue;
        }
        return Ok(SecretAuth::apikey(apikey));
    }
}

fn print_api_key_source_warning(message: &str) {
    if std::io::stderr().is_terminal() {
        eprintln!("\x1b[33mWarning:\x1b[0m {message}");
    } else {
        eprintln!("Warning: {message}");
    }
}

fn read_api_key_file(path: &str) -> Result<String> {
    let contents = std::fs::read_to_string(path)?;
    let mut lines = contents.lines().map(str::trim).filter(|line| !line.is_empty());
    let apikey = lines.next().ok_or_else(|| eyre!("API key file is empty"))?.to_string();
    if lines.next().is_some() {
        return Err(eyre!("API key file must contain exactly one non-empty line"));
    }
    Ok(apikey)
}

#[cfg(feature = "agent")]
#[derive(Debug)]
struct SkillInstallationFailure {
    context: AgentSkillsFailureContext,
}

#[cfg(feature = "agent")]
impl std::fmt::Display for SkillInstallationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "one or more ESDiag skill targets could not be installed")
    }
}

#[cfg(feature = "agent")]
impl std::error::Error for SkillInstallationFailure {}

#[cfg(feature = "agent")]
struct SkillInstallationRun {
    detected: Vec<SkillTarget>,
    selected: Vec<SkillTarget>,
    version: String,
    digest: String,
    results: Vec<AgentSkillTargetResult>,
    failed: bool,
}

#[cfg(feature = "agent")]
async fn run_agent_ask(prompt: String, agent: String, conversation: Option<String>) -> Result<CommandResult> {
    run_agent_ask_for_output(prompt, agent, conversation, None).await
}

#[cfg(feature = "agent")]
async fn run_agent_ask_for_output(
    prompt: String,
    agent: String,
    conversation: Option<String>,
    output_target: Option<&str>,
) -> Result<CommandResult> {
    let deployment = OutputDeployment::resolve(output_target, true)?;
    let viewer = deployment
        .kibana
        .ok_or_else(|| eyre!("The configured output deployment has no Kibana viewer"))?;
    let viewer_auth = deployment
        .kibana_auth
        .ok_or_else(|| eyre!("The configured output deployment has no Kibana viewer authentication"))?;
    let viewer_url = viewer.get_url()?;
    let location = AgentBuilderLocation::new(viewer_url.clone(), agent_builder_space(&viewer_url));
    let client_url = if std::env::var("ESDIAG_CONTAINER_LOCAL_STACK").ok().as_deref() == Some("full") {
        std::env::var("ESDIAG_KIBANA_INTERNAL_URL")
            .ok()
            .map(|url| Url::parse(&url))
            .transpose()?
            .unwrap_or_else(|| viewer_url.clone())
    } else {
        viewer_url.clone()
    };
    let client = KibanaClient::try_new(client_url, viewer_auth)?;
    let agent_client = AgentBuilderClient::new(&client, location);
    let agent_name = agent_client
        .agent_name(&agent)
        .await
        .unwrap_or_else(|_| readable_agent_name(&agent));
    let request = AgentRequest {
        agent_id: agent,
        prompt,
        conversation_id: conversation,
    };
    let completion = agent_client
        .ask(request, |progress| render_agent_progress(&agent_name, progress))
        .await
        .map_err(|error| eyre!(error))?;

    Ok(CommandResult::outcome(CliOutcome::AgentResponse {
        conversation_id: completion.conversation_id,
        message: completion.message,
        kibana_url: completion.kibana_url,
        usage: completion.usage.map(|usage| AgentUsageResult {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
        }),
    }))
}

#[cfg(feature = "agent")]
fn process_ask_prompt(diagnostic_id: &str, prompt: &str) -> String {
    format!("diagnostic.id: {diagnostic_id}\n{prompt}")
}

#[cfg(feature = "agent")]
fn render_agent_progress(agent_name: &str, progress: AgentProgress) {
    match progress {
        AgentProgress::Reasoning(message) | AgentProgress::ToolProgress(message) => {
            eprintln!("{agent_name}: {message}");
        }
        AgentProgress::ToolCall { tool_id } => eprintln!("{agent_name}: started tool {tool_id}"),
        AgentProgress::ToolResult { tool_id } => eprintln!("{agent_name}: completed tool {tool_id}"),
    }
}

#[cfg(feature = "agent")]
fn readable_agent_name(agent_id: &str) -> String {
    let words: Vec<_> = agent_id
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| match word.to_ascii_lowercase().as_str() {
            "ai" => "AI".to_string(),
            "api" => "API".to_string(),
            _ => {
                let mut characters = word.chars();
                let Some(first) = characters.next() else {
                    return String::new();
                };
                format!("{}{}", first.to_uppercase(), characters.as_str())
            }
        })
        .collect();
    if words.is_empty() {
        agent_id.to_string()
    } else {
        words.join(" ")
    }
}

#[cfg(feature = "agent")]
fn agent_builder_space(viewer: &Url) -> Option<String> {
    match std::env::var("ESDIAG_KIBANA_SPACE") {
        Ok(space) => {
            let space = space.trim();
            (!space.is_empty()).then(|| space.to_string())
        }
        Err(std::env::VarError::NotPresent) => {
            let viewer_has_space = viewer.path_segments().is_some_and(|mut segments| {
                while let Some(segment) = segments.next() {
                    if segment == "s" {
                        return segments.next().is_some();
                    }
                }
                false
            });
            (!viewer_has_space).then(|| esdiag::env::ESDIAG_KIBANA_DEFAULT_SPACE.to_string())
        }
        Err(_) => Some(esdiag::env::ESDIAG_KIBANA_DEFAULT_SPACE.to_string()),
    }
}

#[cfg(feature = "agent")]
fn run_agent_skills(targets: Vec<AgentSkillTarget>, force: bool) -> Result<CommandResult> {
    let environment = SkillEnvironment::current();
    let detected = detected_targets(&environment);
    let selected = if targets.is_empty() {
        detected.clone()
    } else {
        let mut targets: Vec<_> = targets.into_iter().map(SkillTarget::from).collect();
        targets.sort_unstable();
        targets.dedup();
        targets
    };
    let installation = install_skill_targets(&environment, detected, selected, force)?;

    if installation.failed {
        return Err(eyre!(SkillInstallationFailure {
            context: AgentSkillsFailureContext {
                detected_targets: installation.detected.iter().map(ToString::to_string).collect(),
                selected_targets: installation.selected.iter().map(ToString::to_string).collect(),
                version: installation.version,
                digest: installation.digest,
                results: installation.results,
                reload_guidance: skill_reload_guidance().to_string(),
            },
        }));
    }
    Ok(CommandResult::outcome(CliOutcome::AgentSkills {
        detected_targets: installation
            .detected
            .into_iter()
            .map(|target| target.to_string())
            .collect(),
        selected_targets: installation
            .selected
            .into_iter()
            .map(|target| target.to_string())
            .collect(),
        version: installation.version,
        digest: installation.digest,
        results: installation.results,
        reload_guidance: skill_reload_guidance().to_string(),
    }))
}

#[cfg(feature = "agent")]
fn install_skill_targets(
    environment: &SkillEnvironment,
    detected: Vec<SkillTarget>,
    selected: Vec<SkillTarget>,
    force: bool,
) -> Result<SkillInstallationRun> {
    let embedded = EmbeddedSkill::current()?;
    let mut results = Vec::with_capacity(selected.len());
    let mut failed = false;

    for target in &selected {
        match target.destination(environment) {
            Err(error) => {
                failed = true;
                results.push(AgentSkillTargetResult {
                    target: target.to_string(),
                    action: "failed".to_string(),
                    destination: "<unavailable>".to_string(),
                    error: Some(error.to_string()),
                });
            }
            Ok(destination) => match install(&destination, &embedded, force) {
                Ok(result) => {
                    let conflict = result.action.as_str() == "conflict";
                    failed |= conflict;
                    results.push(AgentSkillTargetResult {
                        target: target.to_string(),
                        action: result.action.as_str().to_string(),
                        destination: result.destination.display().to_string(),
                        error: conflict.then(|| {
                            "The existing skill is locally modified or unrecognized; rerun with --force to replace it."
                                .to_string()
                        }),
                    });
                }
                Err(error) => {
                    failed = true;
                    results.push(AgentSkillTargetResult {
                        target: target.to_string(),
                        action: "failed".to_string(),
                        destination: destination.display().to_string(),
                        error: Some(error.to_string()),
                    });
                }
            },
        }
    }

    Ok(SkillInstallationRun {
        detected,
        selected,
        version: env!("CARGO_PKG_VERSION").to_string(),
        digest: embedded.digest().to_string(),
        results,
        failed,
    })
}

#[cfg(feature = "agent")]
const fn skill_reload_guidance() -> &'static str {
    "Restart or reload running coding agents before using the installed skill."
}

fn default_diagnostic_user() -> String {
    default_diagnostic_user_from(|name| std::env::var(name).ok())
}

fn default_diagnostic_user_from<F>(get_environment: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    get_environment("EMAIL")
        .filter(|email| email.contains('@'))
        .or_else(|| get_environment("USER"))
        .or_else(|| get_environment("USERNAME"))
        .unwrap_or_else(|| "user".to_string())
}

fn format_epoch(epoch_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(epoch_seconds, 0)
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| epoch_seconds.to_string())
}

fn format_remaining_duration(expires_at_epoch: i64) -> String {
    format_remaining_duration_from(chrono::Utc::now().timestamp(), expires_at_epoch)
}

fn format_remaining_duration_from(now_epoch: i64, expires_at_epoch: i64) -> String {
    let remaining = expires_at_epoch.saturating_sub(now_epoch);
    let duration = Duration::from_secs(remaining.max(0) as u64);
    let days = duration.as_secs() / 86_400;
    let hours = (duration.as_secs() % 86_400) / 3_600;
    let minutes = (duration.as_secs() % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h remaining")
    } else if hours > 0 {
        format!("{hours}h {minutes}m remaining")
    } else {
        format!("{minutes}m remaining")
    }
}

#[cfg(test)]
fn format_keystore_lock_status(status: &esdiag::data::UnlockStatus) -> String {
    format_keystore_lock_status_at(chrono::Utc::now().timestamp(), status)
}

#[cfg(test)]
fn format_keystore_lock_status_at(now_epoch: i64, status: &esdiag::data::UnlockStatus) -> String {
    if status.unlock_active {
        if let Some(expires_at_epoch) = status.expires_at_epoch {
            return format!(
                "Keystore: unlocked until {} ({})",
                format_epoch(expires_at_epoch),
                format_remaining_duration_from(now_epoch, expires_at_epoch)
            );
        }
        return "Keystore: unlocked".to_string();
    }

    "Keystore: locked".to_string()
}

#[cfg(test)]
fn colorize_keystore_lock_status(status: &str, colorize: bool) -> String {
    if !colorize {
        return status.to_string();
    }

    if status.contains("Keystore: unlocked") {
        return status.replacen("unlocked", "\x1b[32munlocked\x1b[0m", 1);
    }
    if status.contains("Keystore: locked") {
        return status.replacen("locked", "\x1b[31mlocked\x1b[0m", 1);
    }
    status.to_string()
}

fn ensure_host_role(host: &KnownHost, role: HostRole, context: &str) -> Result<()> {
    if host.has_role(role.clone()) {
        Ok(())
    } else {
        Err(eyre!(
            "Host role validation failed for {context}: required role '{}' not present",
            role
        ))
    }
}

fn ensure_uri_role(uri: &Uri, role: HostRole, context: &str) -> Result<()> {
    match uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
            ensure_host_role(host, role, context)
        }
        _ => Ok(()),
    }
}

fn resolve_host_secret_auth(secret_id: Option<&str>) -> Result<Option<SecretAuth>> {
    let Some(secret_id) = secret_id else {
        return Ok(None);
    };

    let keystore_password = get_password_for_secret_commands()?;
    let secret_auth = resolve_secret_auth(secret_id, &keystore_password)?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    Ok(Some(secret_auth))
}

fn resolve_same_name_host_secret_auth(host_name: &str) -> Result<Option<SecretAuth>> {
    let Ok(keystore_password) = get_password_for_secret_commands() else {
        return Ok(None);
    };
    resolve_secret_auth(host_name, &keystore_password)
}

fn host_connection_uses_receiver(uri: &Uri) -> bool {
    matches!(uri, Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_))
}

async fn validate_host_connection(name: &str, uri: Uri) -> Result<String> {
    if host_connection_uses_receiver(&uri) {
        let receiver = Receiver::try_from(uri)?;
        if receiver.is_connected().await {
            let summary = format!("Host {name}: connected to Elastic Cloud Admin proxy");
            tracing::info!("{summary}");
            return Ok(summary);
        }

        tracing::error!("Host connection: FAILED ❌ Elastic Cloud Admin proxy connection failed");
        tracing::warn!("Check your URL, certificates, and secret credentials!");
        return Err(eyre!("Host connection failed"));
    }

    match Client::try_from(uri)?.test_connection().await {
        Ok(message) => {
            let summary = format!("Host {name}: {message}");
            tracing::info!("{summary}");
            Ok(summary)
        }
        Err(message) => {
            tracing::error!("Host connection: FAILED ❌ {}", &message);
            tracing::warn!("Check your URL and certificates!");
            Err(eyre!("Host connection failed"))
        }
    }
}

fn should_error_for_missing_subcommand(arg_count: usize, has_no_command: bool) -> bool {
    arg_count > 1 && has_no_command
}

fn clear_last_run_files() -> Result<()> {
    let home_dir = match std::env::consts::OS {
        "windows" => std::env::var("USERPROFILE")?,
        "linux" | "macos" => std::env::var("HOME")?,
        os => return Err(eyre!("Unknown home directory for operating system: {os} ")),
    };
    tracing::debug!("Home directory is: {home_dir}");
    let last_run = std::path::PathBuf::from(home_dir).join(".esdiag/last_run");
    if !last_run.exists() {
        std::fs::create_dir_all(&last_run)?;
    }
    let files = vec![
        "bulk_errors.ndjson",
        "diagnostic.json",
        "report.json",
        "responses.ndjson",
    ];
    for file in files {
        let file = last_run.join(file);
        tracing::debug!("Removing {}", &file.display());
        // Ignore "file not found" errors on delete
        let _ = std::fs::remove_file(file);
    }
    Ok(())
}

#[cfg(feature = "server")]
fn resolve_serve_runtime_mode(mode: Option<RuntimeMode>) -> Result<RuntimeMode> {
    if let Some(mode) = mode {
        return Ok(mode);
    }
    match std::env::var("ESDIAG_MODE") {
        Ok(value) => RuntimeMode::from_env(&value),
        Err(std::env::VarError::NotPresent) => Ok(RuntimeMode::User),
        Err(err) => Err(eyre!("Failed to read ESDIAG_MODE: {err}")),
    }
}

#[cfg(feature = "server")]
fn resolve_serve_exporter(output: Option<String>) -> Result<Exporter> {
    Exporter::try_from(Uri::try_from(output)?)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "agent")]
    use super::{
        AgentCommands, AgentSkillTarget, SkillInstallationFailure, agent_builder_space, install_skill_targets,
        process_ask_prompt, readable_agent_name,
    };
    use super::{
        Cli, Commands, HostCommands, KeystoreCommands, classify_failure, colorize_keystore_lock_status,
        command_owns_stdout, default_diagnostic_user_from, detected_esdiag_local_preset,
        ensure_output_deployment_valid, format_keystore_lock_status, format_keystore_lock_status_at,
        format_remaining_duration_from, host_connection_uses_receiver, is_agent_mode, local_core_stack_start_args,
        local_stack_outcome, resolve_host_secret_auth, resolve_secret_input_with_prompt, resolve_tracing_filter,
        should_error_for_missing_subcommand, should_run_output_setup, should_start_local_core_stack,
        structured_failure,
    };
    #[cfg(feature = "keystore")]
    use super::{derive_collect_job, derive_process_job};
    #[cfg(feature = "server")]
    use super::{resolve_serve_exporter, resolve_serve_runtime_mode};
    use clap::Parser;
    use esdiag::data::{Application, HostRole, KnownHost, SecretAuth, UnlockStatus, Uri, upsert_secret_auth};
    #[cfg(feature = "server")]
    use esdiag::server::RuntimeMode;
    #[cfg(feature = "agent")]
    use esdiag::{
        agent::{
            builder::AgentFailure,
            skills::{SkillEnvironment, SkillTarget},
        },
        cli_output::{AgentSkillTargetResult, AgentSkillsFailureContext},
    };
    use esdiag::{
        cli_output::{CliFailureCategory, OutputFormat},
        receiver::{
            ElasticCloudAdminRequestError, ElasticsearchRequestError, KibanaRequestError, LogstashRequestError,
        },
    };
    #[cfg(feature = "keystore")]
    use esdiag::{
        job::model::{ExportTarget, Input, Job, Process},
        processor::Identifiers,
    };
    use std::{ffi::OsString, sync::Mutex};
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        let keystore = tmp.path().join("secrets.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore);
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }
        tmp
    }

    #[test]
    fn no_args_and_no_command_allows_desktop_path() {
        assert!(!should_error_for_missing_subcommand(1, true));
    }

    #[test]
    fn args_without_subcommand_errors() {
        assert!(should_error_for_missing_subcommand(2, true));
        assert!(should_error_for_missing_subcommand(3, true));
    }

    #[test]
    fn args_with_subcommand_does_not_error() {
        assert!(!should_error_for_missing_subcommand(2, false));
    }

    #[test]
    fn agent_flag_parses_long_and_short_forms() {
        let cli = Cli::parse_from(["esdiag", "--agent", "keystore", "status"]);
        assert!(cli.agent, "long --agent should enable agent mode");

        let cli = Cli::parse_from(["esdiag", "-a", "keystore", "status"]);
        assert!(cli.agent, "short -a should enable agent mode");
    }

    #[cfg(not(feature = "agent"))]
    #[test]
    fn agent_commands_require_the_agent_feature() {
        assert!(Cli::try_parse_from(["esdiag", "agent", "ask", "Analyze"]).is_err());
        assert!(Cli::try_parse_from(["esdiag", "process", "diagnostic.zip", "--ask", "Analyze"]).is_err());
    }

    #[test]
    fn output_format_defaults_to_yaml_and_accepts_json() {
        let yaml = Cli::parse_from(["esdiag", "keystore", "status"]);
        assert_eq!(yaml.format, OutputFormat::Yaml);

        let json = Cli::parse_from(["esdiag", "--format", "json", "keystore", "status"]);
        assert_eq!(json.format, OutputFormat::Json);
    }

    #[cfg(feature = "agent")]
    #[test]
    fn agent_ask_accepts_explicit_follow_up_or_new_conversation() {
        let follow_up = Cli::parse_from([
            "esdiag",
            "agent",
            "ask",
            "--conversation",
            "conv-123",
            "Explain further",
        ]);
        assert!(matches!(
            follow_up.command,
            Some(Commands::Agent {
                command: AgentCommands::Ask {
                    conversation: Some(conversation),
                    new: false,
                    ..
                }
            }) if conversation == "conv-123"
        ));

        let new = Cli::parse_from(["esdiag", "agent", "ask", "--new", "Start a fresh analysis"]);
        assert!(matches!(
            new.command,
            Some(Commands::Agent {
                command: AgentCommands::Ask {
                    conversation: None,
                    new: true,
                    ..
                }
            })
        ));
        let override_agent = Cli::parse_from(["esdiag", "agent", "ask", "--agent", "custom-agent", "Analyze"]);
        assert!(matches!(
            override_agent.command,
            Some(Commands::Agent {
                command: AgentCommands::Ask {
                    agent_id,
                    ..
                }
            }) if agent_id == "custom-agent"
        ));
        assert!(
            Cli::try_parse_from([
                "esdiag",
                "agent",
                "ask",
                "--new",
                "--conversation",
                "conv-123",
                "invalid",
            ])
            .is_err()
        );
    }

    #[cfg(feature = "agent")]
    #[test]
    fn process_ask_prefixes_the_completed_diagnostic_id() {
        let cli = Cli::parse_from([
            "esdiag",
            "process",
            "diagnostic.zip",
            "output-es",
            "--ask",
            "What is the highest-risk finding?",
        ]);
        assert!(matches!(
            cli.command,
            Some(Commands::Process {
                ask: Some(prompt),
                ..
            }) if prompt == "What is the highest-risk finding?"
        ));
        assert_eq!(
            process_ask_prompt("prod-es@2026-08-18~a1b2", "What is the highest-risk finding?"),
            "diagnostic.id: prod-es@2026-08-18~a1b2\nWhat is the highest-risk finding?"
        );
    }

    #[cfg(feature = "agent")]
    #[test]
    fn unreadable_agent_names_fall_back_to_a_readable_agent_id() {
        assert_eq!(readable_agent_name("diagnostic-agent"), "Diagnostic Agent");
        assert_eq!(readable_agent_name("elastic-ai-agent"), "Elastic AI Agent");
        assert_eq!(readable_agent_name("custom_api_agent"), "Custom API Agent");
    }

    #[cfg(feature = "agent")]
    #[test]
    fn agent_builder_space_requires_a_space_segment_pair() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }

        assert_eq!(
            agent_builder_space(&Url::parse("https://kb.example/app/s").expect("url")),
            Some("esdiag".to_string())
        );
        assert_eq!(
            agent_builder_space(&Url::parse("https://kb.example/app/s/support").expect("url")),
            None
        );
    }

    #[cfg(feature = "agent")]
    #[test]
    fn agent_skills_allows_repeatable_explicit_targets() {
        let cli = Cli::parse_from([
            "esdiag", "agent", "skills", "--target", "claude", "--target", "opencode", "--force",
        ]);
        assert!(matches!(
            cli.command,
            Some(Commands::Agent {
                command: AgentCommands::Skills {
                    target,
                    force: true,
                }
            }) if matches!(target.as_slice(), [AgentSkillTarget::Claude, AgentSkillTarget::OpenCode])
        ));
    }

    #[cfg(feature = "agent")]
    #[test]
    fn embedded_skill_installs_into_an_explicit_target_without_agent_detection() {
        let temporary = tempfile::tempdir().expect("temporary home");
        let environment = SkillEnvironment {
            home: Some(temporary.path().to_path_buf()),
            ..SkillEnvironment::default()
        };

        let installation = install_skill_targets(&environment, vec![], vec![SkillTarget::Codex], false)
            .expect("embedded skill installation");

        assert!(!installation.failed);
        assert_eq!(installation.results.len(), 1);
        assert_eq!(installation.results[0].action, "installed");
        assert!(
            temporary.path().join(".agents/skills/esdiag/SKILL.md").is_file(),
            "explicit target must install even when it was not detected"
        );
    }

    #[cfg(feature = "agent")]
    #[test]
    fn failed_agent_skill_installation_preserves_complete_structured_context() {
        let error = eyre::Report::new(SkillInstallationFailure {
            context: AgentSkillsFailureContext {
                detected_targets: vec!["codex".to_string()],
                selected_targets: vec!["claude".to_string(), "codex".to_string()],
                version: "0.17.0-SNAPSHOT".to_string(),
                digest: "a1b2".to_string(),
                results: vec![
                    AgentSkillTargetResult {
                        target: "claude".to_string(),
                        action: "installed".to_string(),
                        destination: "/tmp/.claude/skills/esdiag".to_string(),
                        error: None,
                    },
                    AgentSkillTargetResult {
                        target: "codex".to_string(),
                        action: "conflict".to_string(),
                        destination: "/tmp/.agents/skills/esdiag".to_string(),
                        error: Some("requires --force".to_string()),
                    },
                ],
                reload_guidance: "Restart or reload running coding agents before using the installed skill."
                    .to_string(),
            },
        });

        let value = serde_json::to_value(structured_failure(&error)).expect("serialize failure");
        let context = &value["agent_skills"];
        assert_eq!(context["detected_targets"], serde_json::json!(["codex"]));
        assert_eq!(context["selected_targets"], serde_json::json!(["claude", "codex"]));
        assert_eq!(context["version"], "0.17.0-SNAPSHOT");
        assert_eq!(context["digest"], "a1b2");
        assert_eq!(context["results"][0]["action"], "installed");
        assert_eq!(context["results"][1]["action"], "conflict");
        assert_eq!(
            context["reload_guidance"],
            "Restart or reload running coding agents before using the installed skill."
        );
        assert_eq!(value["target_results"][0]["action"], "installed");
        assert_eq!(value["target_results"][1]["action"], "conflict");
    }

    #[cfg(feature = "agent")]
    #[test]
    fn interrupted_agent_conversation_exposes_only_safe_recovery() {
        let error = eyre::Report::new(AgentFailure::Interrupted {
            conversation_id: Some("conv-123".to_string()),
            kibana_url: Some("https://kb.example/app/agent_builder/conversations/conv-123".to_string()),
        });

        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["recovery"]["conversation_id"], "conv-123");
        assert_eq!(value["recovery"]["retry_safe"], false);
        assert_eq!(
            value["recovery"]["kibana_url"],
            "https://kb.example/app/agent_builder/conversations/conv-123"
        );
    }

    #[test]
    fn default_diagnostic_user_uses_environment_without_spawning_processes() {
        let email = default_diagnostic_user_from(|name| match name {
            "EMAIL" => Some("operator@example.com".to_string()),
            "USER" => Some("shell-user".to_string()),
            _ => None,
        });
        let user = default_diagnostic_user_from(|name| match name {
            "USER" => Some("shell-user".to_string()),
            _ => None,
        });

        assert_eq!(email, "operator@example.com");
        assert_eq!(user, "shell-user");
    }

    #[test]
    fn unauthorized_elasticsearch_responses_are_authentication_failures() {
        let error = eyre::Report::new(ElasticsearchRequestError::new(
            elasticsearch::http::StatusCode::UNAUTHORIZED,
            "missing authentication credentials".to_string(),
            5,
            34,
        ));

        assert_eq!(classify_failure(&error), CliFailureCategory::AuthenticationFailed);
    }

    #[test]
    fn http_failures_include_status_type_and_reason() {
        let error = eyre::Report::new(ElasticsearchRequestError::new(
            elasticsearch::http::StatusCode::UNAUTHORIZED,
            r#"{"error":{"type":"security_exception","reason":"missing authentication credentials for REST request [api_key=secret]"},"status":401}"#
                .to_string(),
            5,
            133,
        ));
        let failure = structured_failure(&error);
        let value = serde_json::to_value(failure).expect("serialize failure");

        assert_eq!(value["category"], CliFailureCategory::AuthenticationFailed.as_str());
        assert_eq!(value["status"], 401);
        assert_eq!(value["type"], "security_exception");
        assert_eq!(
            value["reason"],
            "missing authentication credentials for REST request [api_key=secret]"
        );
        assert_eq!(
            value["message"],
            "The server rejected the request because authentication credentials are required."
        );
    }

    #[test]
    fn non_elasticsearch_http_failures_use_the_shared_response_projection() {
        let body = r#"{"error":{"type":"upstream_failure","reason":"remote dependency was unavailable"}}"#.to_string();
        let failures = [
            (
                eyre::Report::new(KibanaRequestError::new(
                    reqwest::StatusCode::TOO_MANY_REQUESTS,
                    body.clone(),
                    5,
                    80,
                )),
                429,
                CliFailureCategory::InvalidInput,
            ),
            (
                eyre::Report::new(LogstashRequestError::new(
                    reqwest::StatusCode::NOT_FOUND,
                    body.clone(),
                    5,
                    80,
                )),
                404,
                CliFailureCategory::NotFound,
            ),
            (
                eyre::Report::new(ElasticCloudAdminRequestError::new(
                    reqwest::StatusCode::BAD_GATEWAY,
                    body,
                    5,
                    80,
                )),
                502,
                CliFailureCategory::Internal,
            ),
        ];

        for (error, status, category) in failures {
            let value = serde_json::to_value(structured_failure(&error)).expect("serialize failure");
            assert_eq!(value["category"], category.as_str());
            assert_eq!(value["status"], status);
            assert_eq!(value["type"], "upstream_failure");
            assert_eq!(value["reason"], "remote dependency was unavailable");
        }
    }

    #[test]
    fn direct_process_stdout_is_not_replaced_with_a_terminal_outcome() {
        let cli = Cli::parse_from(["esdiag", "process", "input.zip", "-"]);
        assert!(command_owns_stdout(&cli));

        #[cfg(feature = "agent")]
        {
            let cli = Cli::parse_from(["esdiag", "process", "input.zip", "-", "--ask", "What changed?"]);
            assert!(!command_owns_stdout(&cli));
        }
    }

    #[test]
    fn local_help_stdout_is_not_replaced_with_a_terminal_outcome() {
        let cli = Cli::parse_from(["esdiag", "local"]);
        assert!(command_owns_stdout(&cli));
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn saved_job_stdout_is_not_replaced_with_a_terminal_outcome() {
        let _env_guard = env_lock().lock().expect("lock environment");
        let _tmp = setup_env();
        let job = Job::try_new(
            Identifiers::default(),
            Input::Collect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(Process {
                selection: None,
                export: ExportTarget::Stdout,
            }),
            None,
        )
        .expect("valid streaming job");
        esdiag::job::save_job("stdout-job", job).expect("save job");

        let cli = Cli::parse_from(["esdiag", "job", "run", "stdout-job"]);
        assert!(command_owns_stdout(&cli));
    }

    #[test]
    fn host_add_parses_comma_delimited_role_values() {
        let cli = Cli::parse_from([
            "esdiag",
            "host",
            "add",
            "prod-es",
            "http://localhost:9200",
            "--app",
            "elasticsearch",
            "--roles",
            "collect,send",
        ]);
        match cli.command.expect("command") {
            Commands::Host {
                command: HostCommands::Add { args, .. },
            } => {
                assert_eq!(args.roles, Some(vec![HostRole::Collect, HostRole::Send]));
            }
            _ => panic!("expected host command"),
        }
    }

    #[test]
    fn parses_init_command() {
        let cli = Cli::try_parse_from(["esdiag", "init"]).expect("parse init");

        assert!(matches!(cli.command, Some(Commands::Init)));
    }

    #[test]
    fn local_command_preserves_lifecycle_arguments() {
        let cli = Cli::try_parse_from(["esdiag", "local", "up", "--stack=core", "--pull", "never"])
            .expect("parse local command");

        match cli.command {
            Some(Commands::Local { args }) => {
                assert_eq!(
                    args,
                    vec![
                        OsString::from("up"),
                        OsString::from("--stack=core"),
                        OsString::from("--pull"),
                        OsString::from("never")
                    ]
                );
            }
            other => panic!("expected local command, got {other:?}"),
        }
    }

    #[test]
    fn init_only_starts_the_rust_core_lifecycle_after_explicit_approval() {
        assert!(should_start_local_core_stack(true, true, || Ok(true)).expect("approved start"));
        assert!(!should_start_local_core_stack(true, true, || Ok(false)).expect("declined start"));
        let prompted = std::cell::Cell::new(false);
        assert!(
            !should_start_local_core_stack(false, true, || {
                prompted.set(true);
                Ok(true)
            })
            .expect("existing stack skips prompt")
        );
        assert!(!prompted.get());
        assert!(
            !should_start_local_core_stack(true, false, || {
                prompted.set(true);
                Ok(true)
            })
            .expect("missing runtime skips prompt")
        );
        assert!(!prompted.get());
        assert_eq!(
            local_core_stack_start_args(),
            vec![
                OsString::from("up"),
                OsString::from("--stack=core"),
                OsString::from("--open-browser=false"),
            ]
        );
    }

    #[test]
    fn init_skips_redundant_setup_for_a_new_local_stack() {
        assert!(
            !should_run_output_setup(true, || { panic!("new local stack must not repeat the asset prompt") })
                .expect("local stack setup is complete")
        );
        assert!(should_run_output_setup(false, || Ok(true)).expect("existing stack approval"));
        assert!(!should_run_output_setup(false, || Ok(false)).expect("existing stack decline"));
    }

    #[test]
    fn invalid_output_endpoints_are_rejected_before_persistence() {
        assert!(ensure_output_deployment_valid(true, true).is_ok());
        for (elasticsearch, kibana) in [(false, false), (false, true), (true, false)] {
            let error = ensure_output_deployment_valid(elasticsearch, kibana).expect_err("invalid output");
            assert!(
                error
                    .to_string()
                    .contains("Existing output configuration was not changed.")
            );
        }
    }

    #[test]
    fn local_preset_accepts_shared_core_state_and_rejects_unknown_modes() {
        let _guard = env_lock().lock().expect("environment lock");
        let state = TempDir::new().expect("temporary local state");
        std::fs::write(
            state.path().join(".env"),
            "STACK_MODE=core\nESDIAG_ELASTICSEARCH_PORT=19200\nESDIAG_KIBANA_PORT=15601\nESDIAG_PORT=12501\nESDIAG_OUTPUT_APIKEY=local-key\n",
        )
        .expect("write local state");
        unsafe {
            std::env::set_var("ESDIAG_LOCAL_DIR", state.path());
        }

        let preset = detected_esdiag_local_preset().expect("read core local state");
        assert_eq!(preset.elasticsearch_url, "http://localhost:19200");
        assert_eq!(preset.kibana_url, "http://localhost:15601");
        assert_eq!(preset.kibana_api_url, "http://localhost:15601");
        assert_eq!(preset.apikey.as_deref(), Some("local-key"));
        let outcome = serde_json::to_string(&local_stack_outcome("status".to_string(), &[OsString::from("status")]))
            .expect("serialize local outcome");
        assert!(outcome.contains("\"mode\":\"core\""));
        assert!(outcome.contains("http://127.0.0.1:12501"));
        assert!(!outcome.contains("local-key"));

        let forwarded_state = TempDir::new().expect("temporary forwarded local state");
        std::fs::write(
            forwarded_state.path().join(".env"),
            "STACK_MODE=full\nESDIAG_PORT=22501\nESDIAG_KIBANA_PORT=25601\n",
        )
        .expect("write forwarded local state");
        for args in [
            vec![
                OsString::from("status"),
                OsString::from("--state-dir"),
                forwarded_state.path().as_os_str().to_os_string(),
            ],
            vec![
                OsString::from("status"),
                OsString::from(format!("--state-dir={}", forwarded_state.path().display())),
            ],
        ] {
            let outcome = serde_json::to_string(&local_stack_outcome("status".to_string(), &args))
                .expect("serialize forwarded local outcome");
            assert!(outcome.contains("\"mode\":\"full\""));
            assert!(outcome.contains("http://127.0.0.1:22501"));
            assert!(outcome.contains("http://127.0.0.1:25601"));
        }

        std::fs::write(state.path().join(".env"), "STACK_MODE=unknown\n").expect("write unsupported state");
        assert!(detected_esdiag_local_preset().is_none());
        unsafe {
            std::env::remove_var("ESDIAG_LOCAL_DIR");
        }
    }

    #[test]
    fn managed_full_container_preset_separates_internal_and_public_kibana_urls() {
        let _guard = env_lock().lock().expect("environment lock");
        unsafe {
            std::env::set_var("ESDIAG_CONTAINER_LOCAL_STACK", "full");
            std::env::set_var("ESDIAG_OUTPUT_URL", "http://elasticsearch:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "container-key");
            std::env::set_var("ESDIAG_KIBANA_INTERNAL_URL", "http://kibana:5601/s/esdiag");
            std::env::set_var("ESDIAG_KIBANA_PUBLIC_URL", "http://127.0.0.1:5601/s/esdiag");
        }

        let preset = detected_esdiag_local_preset().expect("read full container state");
        assert_eq!(preset.elasticsearch_url, "http://elasticsearch:9200");
        assert_eq!(preset.kibana_api_url, "http://kibana:5601/s/esdiag");
        assert_eq!(preset.kibana_url, "http://127.0.0.1:5601/s/esdiag");
        assert_eq!(preset.apikey.as_deref(), Some("container-key"));

        for variable in [
            "ESDIAG_CONTAINER_LOCAL_STACK",
            "ESDIAG_OUTPUT_URL",
            "ESDIAG_OUTPUT_APIKEY",
            "ESDIAG_KIBANA_INTERNAL_URL",
            "ESDIAG_KIBANA_PUBLIC_URL",
        ] {
            unsafe {
                std::env::remove_var(variable);
            }
        }
    }

    #[test]
    fn host_update_accept_invalid_certs_cli_parses_explicit_bool_values() {
        let cli_true = Cli::parse_from(["esdiag", "host", "update", "prod-es", "--accept-invalid-certs", "true"]);
        match cli_true.command.expect("command") {
            Commands::Host {
                command: HostCommands::Update { args, .. },
            } => {
                assert_eq!(args.accept_invalid_certs, Some(true));
            }
            _ => panic!("expected host command"),
        }

        let cli_false = Cli::parse_from(["esdiag", "host", "update", "prod-es", "--accept-invalid-certs", "false"]);
        match cli_false.command.expect("command") {
            Commands::Host {
                command: HostCommands::Update { args, .. },
            } => {
                assert_eq!(args.accept_invalid_certs, Some(false));
            }
            _ => panic!("expected host command"),
        }
    }

    #[test]
    fn host_legacy_positional_syntax_is_captured_as_legacy_subcommand() {
        let cli = Cli::parse_from(["esdiag", "host", "prod-es", "--secret", "rotated"]);
        match cli.command.expect("command") {
            Commands::Host {
                command: HostCommands::Legacy(args),
            } => {
                assert_eq!(
                    args,
                    vec!["prod-es".to_string(), "--secret".to_string(), "rotated".to_string()]
                );
            }
            _ => panic!("expected legacy host command"),
        }
    }

    #[test]
    fn keystore_add_allows_missing_apikey_value_for_prompting() {
        let cli = Cli::parse_from(["esdiag", "keystore", "add", "prod-es", "--apikey"]);
        match cli.command.expect("command") {
            Commands::Keystore {
                command: KeystoreCommands::Add { apikey, .. },
            } => {
                assert_eq!(apikey, Some(String::new()));
            }
            _ => panic!("expected keystore add command"),
        }
    }

    #[test]
    fn keystore_update_allows_missing_password_value_for_prompting() {
        let cli = Cli::parse_from([
            "esdiag",
            "keystore",
            "update",
            "prod-es",
            "--user",
            "elastic",
            "--password",
        ]);
        match cli.command.expect("command") {
            Commands::Keystore {
                command: KeystoreCommands::Update { password, .. },
            } => {
                assert_eq!(password, Some(String::new()));
            }
            _ => panic!("expected keystore update command"),
        }
    }

    #[test]
    fn resolve_secret_input_uses_prompted_value_for_missing_apikey() {
        let mut prompts = Vec::new();
        let resolved = resolve_secret_input_with_prompt(None, None, Some(String::new()), |prompt| {
            prompts.push(prompt.to_string());
            Ok("prompted-api-key".to_string())
        })
        .expect("resolve secret input");

        assert_eq!(prompts, vec!["Enter secret API key: ".to_string()]);
        assert_eq!(resolved, (None, None, Some("prompted-api-key".to_string())));
    }

    #[test]
    fn remaining_duration_formats_hours_and_minutes() {
        assert_eq!(
            format_remaining_duration_from(1_700_000_000, 1_700_003_660),
            "1h 1m remaining"
        );
    }

    #[test]
    fn keystore_status_reports_locked_state() {
        let status = UnlockStatus {
            keystore_exists: true,
            unlock_active: false,
            expires_at_epoch: None,
            unlock_path: std::path::PathBuf::from("/tmp/keystore.unlock"),
        };

        assert_eq!(format_keystore_lock_status(&status), "Keystore: locked");
    }

    #[test]
    fn keystore_status_reports_unlock_expiry() {
        let status = UnlockStatus {
            keystore_exists: true,
            unlock_active: true,
            expires_at_epoch: Some(1_700_003_660),
            unlock_path: std::path::PathBuf::from("/tmp/keystore.unlock"),
        };

        assert_eq!(
            format_keystore_lock_status_at(1_700_000_000, &status),
            "Keystore: unlocked until 2023-11-14T23:14:20+00:00 (1h 1m remaining)"
        );
    }

    #[test]
    fn keystore_status_colorizes_locked_state_when_enabled() {
        assert_eq!(
            colorize_keystore_lock_status("Keystore: locked", true),
            "Keystore: \u{1b}[31mlocked\u{1b}[0m"
        );
    }

    #[test]
    fn keystore_status_colorizes_unlocked_state_when_enabled() {
        assert_eq!(
            colorize_keystore_lock_status("Keystore: unlocked until later", true),
            "Keystore: \u{1b}[32munlocked\u{1b}[0m until later"
        );
    }

    #[test]
    fn keystore_status_leaves_plain_text_when_color_disabled() {
        assert_eq!(
            colorize_keystore_lock_status("Keystore: locked", false),
            "Keystore: locked"
        );
    }

    #[test]
    fn collect_command_parses_upload_id() {
        let cli = Cli::parse_from(["esdiag", "collect", "prod-es", "diag-dir", "--upload", "abc123"]);
        match cli.command.expect("command") {
            Commands::Collect {
                host,
                output,
                upload_id,
                user,
                ..
            } => {
                assert_eq!(host, "prod-es");
                assert_eq!(output, "diag-dir");
                assert_eq!(upload_id, Some("abc123".to_string()));
                assert_eq!(user, None);
            }
            _ => panic!("expected collect command"),
        }
    }

    #[test]
    fn collect_command_keeps_user_short_option() {
        let cli = Cli::parse_from([
            "esdiag", "collect", "prod-es", "diag-dir", "-u", "elastic", "--upload", "abc123",
        ]);
        match cli.command.expect("command") {
            Commands::Collect { upload_id, user, .. } => {
                assert_eq!(user, Some("elastic".to_string()));
                assert_eq!(upload_id, Some("abc123".to_string()));
            }
            _ => panic!("expected collect command"),
        }
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn collect_command_parses_save_job_flag() {
        let cli = Cli::parse_from(["esdiag", "collect", "prod-es", "diag-dir", "--save-job", "nightly-prod"]);
        match cli.command.expect("command") {
            Commands::Collect { save_job, .. } => {
                assert_eq!(save_job.as_deref(), Some("nightly-prod"));
            }
            _ => panic!("expected collect command"),
        }
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn process_command_parses_save_job_flag() {
        let cli = Cli::parse_from([
            "esdiag",
            "process",
            "prod-es",
            "monitoring-es",
            "--save-job",
            "process-prod",
        ]);
        match cli.command.expect("command") {
            Commands::Process { save_job, .. } => {
                assert_eq!(save_job.as_deref(), Some("process-prod"));
            }
            _ => panic!("expected process command"),
        }
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn derive_collect_job_requires_known_host_input() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        let err = match derive_collect_job(
            "https://example.com",
            "diag-dir",
            "standard",
            None,
            Identifiers::default(),
        ) {
            Ok(_) => panic!("non-host input should be rejected"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("Jobs require a saved known host name as input")
        );
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn derive_collect_job_uses_output_dir_without_save_dir() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        let host = KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        );
        host.save("prod-es").expect("save known host");

        let job = derive_collect_job("prod-es", "/tmp/esdiag-output", "support", None, Identifiers::default())
            .expect("derive collect job");

        assert!(job.process().is_none());
        assert!(job.send().is_none());
        assert_eq!(
            job.save().and_then(|save| save.dir.as_ref()),
            Some(&std::path::PathBuf::from("/tmp/esdiag-output"))
        );
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn derive_process_job_requires_explicit_output() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        let host = KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        );
        host.save("prod-es").expect("save known host");
        let err = match derive_process_job("prod-es", None, Identifiers::default()) {
            Ok(_) => panic!("missing process output should be rejected"),
            Err(err) => err,
        };
        assert_eq!(err.to_string(), "Saved jobs require an explicit process output target");
    }

    #[test]
    fn upload_command_parses_file_and_upload_id() {
        let cli = Cli::parse_from(["esdiag", "upload", "diag.zip", "abc123"]);
        match cli.command.expect("command") {
            Commands::Upload {
                file_name,
                upload_id,
                api_url,
            } => {
                assert_eq!(file_name, "diag.zip");
                assert_eq!(upload_id, "abc123");
                assert_eq!(api_url, esdiag::uploader::DEFAULT_UPLOAD_API_URL);
            }
            _ => panic!("expected upload command"),
        }
    }

    #[test]
    fn agent_mode_auto_enables_from_claudecode_env() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("CLAUDECODE", "1");
        }

        let cli = Cli::parse_from(["esdiag", "keystore", "status"]);
        assert!(is_agent_mode(&cli));

        unsafe {
            std::env::remove_var("CLAUDECODE");
        }
    }

    #[test]
    fn debug_overrides_agent_warn_filter() {
        let cli = Cli {
            debug: true,
            agent: true,
            format: OutputFormat::Yaml,
            command: None,
        };

        assert_eq!(resolve_tracing_filter(&cli).to_string(), "debug");
    }

    #[test]
    fn agent_mode_uses_warn_filter_without_debug() {
        let cli = Cli {
            debug: false,
            agent: true,
            format: OutputFormat::Yaml,
            command: None,
        };

        assert_eq!(resolve_tracing_filter(&cli).to_string(), "warn");
    }

    #[test]
    fn elastic_cloud_admin_hosts_validate_via_receiver() {
        let uri = Uri::ElasticCloudAdmin(KnownHost::new_legacy_apikey(
            Application::Elasticsearch,
            Url::parse("https://admin.found.no/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/")
                .expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
            Some("ada-admin".to_string()),
            None,
        ));

        assert!(host_connection_uses_receiver(&uri));
    }

    #[test]
    fn standard_known_hosts_validate_via_client() {
        let uri = Uri::KnownHost(KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        ));

        assert!(!host_connection_uses_receiver(&uri));
    }

    #[test]
    fn host_secret_auth_resolution_detects_apikey() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        upsert_secret_auth("api-secret", SecretAuth::apikey("secret-key"), "pw").expect("save api secret");

        let resolved = resolve_host_secret_auth(Some("api-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_detects_basic() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        upsert_secret_auth("basic-secret", SecretAuth::basic("elastic", "secret-password"), "pw")
            .expect("save basic secret");

        let resolved = resolve_host_secret_auth(Some("basic-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::Basic { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_reads_named_secret() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        upsert_secret_auth("host-fallback", SecretAuth::apikey("secret-key"), "pw").expect("save fallback secret");

        let resolved = resolve_host_secret_auth(Some("host-fallback")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_prefers_explicit_flag_over_env() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_MODE", "service");
        }

        let resolved = resolve_serve_runtime_mode(Some(RuntimeMode::User)).expect("resolve mode");

        assert_eq!(resolved, RuntimeMode::User);

        unsafe {
            std::env::remove_var("ESDIAG_MODE");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_uses_env_when_flag_missing() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_MODE", "service");
        }

        let resolved = resolve_serve_runtime_mode(None).expect("resolve mode");

        assert_eq!(resolved, RuntimeMode::Service);

        unsafe {
            std::env::remove_var("ESDIAG_MODE");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_defaults_to_user_without_flag_or_env() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_MODE");
        }

        let resolved = resolve_serve_runtime_mode(None).expect("resolve mode");

        assert_eq!(resolved, RuntimeMode::User);
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_parses_web_features_flag() {
        let cli = Cli::parse_from(["esdiag", "serve", "--web-features", "advanced,job-builder"]);

        match cli.command {
            Some(Commands::Serve { web_features, .. }) => {
                assert_eq!(web_features.as_deref(), Some("advanced,job-builder"));
            }
            other => panic!("expected serve command, got {other:?}"),
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_requires_configuration_when_output_is_omitted() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let err = match resolve_serve_exporter(None) {
            Ok(_) => panic!("omitted output without a configured deployment must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("No output deployment is configured"));
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_explicit_output_precedes_runtime_environment() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "runtime-secret");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let exporter = resolve_serve_exporter(Some("-".to_string())).expect("resolve explicit output");

        assert_eq!(exporter.target_uri(), "stdio://stdout");

        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_uses_runtime_environment_when_output_is_omitted() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "http://localhost:9200");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "runtime-secret");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let exporter = resolve_serve_exporter(None).expect("resolve runtime output");

        assert_eq!(exporter.target_uri(), "http://localhost:9200/");

        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_rejects_partial_runtime_environment_without_leaking_secrets() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_OUTPUT_URL", "http://localhost:9200");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::set_var("ESDIAG_OUTPUT_USERNAME", "do-not-print-this-secret");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let err = match resolve_serve_exporter(None) {
            Ok(_) => panic!("partial output must fail closed"),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(message.contains("ESDIAG_OUTPUT_USERNAME and ESDIAG_OUTPUT_PASSWORD"));
        assert!(!message.contains("do-not-print-this-secret"));

        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
        }
    }
}

#[cfg(all(test, feature = "server", feature = "desktop"))]
mod desktop_startup_tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn embedded_server_starts_and_serves_local_url() {
        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
            .await
            .expect("desktop embedded server should start");

        let url = format!("http://localhost:{}", bound_addr.port());
        let parsed = tauri::Url::parse(&url).expect("desktop URL should be valid");

        let response = reqwest::get(parsed.as_str())
            .await
            .expect("embedded server should accept HTTP requests");
        assert!(
            response.status().is_success(),
            "expected success status from embedded server, got {}",
            response.status()
        );

        server.shutdown().await;
    }

    #[tokio::test]
    async fn embedded_server_avoids_occupied_port_by_using_ephemeral_binding() {
        let occupied_listener = TcpListener::bind("127.0.0.1:0").expect("should reserve a local test port");
        let occupied_port = occupied_listener
            .local_addr()
            .expect("reserved listener has local addr")
            .port();

        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
            .await
            .expect("desktop embedded server should start while another port is occupied");

        assert_ne!(
            bound_addr.port(),
            occupied_port,
            "ephemeral bind should avoid occupied ports"
        );

        let url = format!("http://localhost:{}", bound_addr.port());
        let response = reqwest::get(&url)
            .await
            .expect("embedded server should accept HTTP requests");
        assert!(
            response.status().is_success(),
            "expected success status from embedded server, got {}",
            response.status()
        );

        server.shutdown().await;
        drop(occupied_listener);
    }
}
