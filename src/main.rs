#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
mod local;
// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{
    ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand, builder::BoolishValueParser, builder::styling,
    error::ErrorKind,
};
#[cfg(feature = "agent")]
use esdiag::cli_output::{AgentSkillTargetResult, AgentSkillsFailureContext};
use esdiag::cli_output::{
    AgentSkillsResult, CliOutcome, CommandOutcome, KeystoreOperation, OutputFormat, command_failure,
    write_terminal_outcome,
};
use esdiag::operations;
#[cfg(feature = "server")]
use esdiag::server::{AuthProvider, RuntimeMode, Server, ServerStartOptions};
#[cfg(feature = "setup")]
use esdiag::setup;
#[cfg(feature = "agent")]
use esdiag::{
    agent::builder::AgentProgress,
    setup::skills::{
        EmbeddedSkill, SkillEnvironment, SkillInstallationFailure, SkillTarget, detected_targets, install,
    },
};
use esdiag::{
    client::Client,
    data::{
        Application, HostRole, KnownHost, KnownHostCliUpdate, OnboardingWorkflow, SecretAuth, Uri, default_unlock_ttl,
        get_password_for_secret_commands, keystore_exists, list_secret_names, parse_unlock_ttl,
        rotate_keystore_password, validate_existing_keystore_password,
    },
    elastic_upload_service,
    env::LOG_LEVEL,
    exporter::Exporter,
    onboarding::{
        CollectHostInput, OutputDeploymentInput, inspect as inspect_onboarding, save_collect_host, save_default_job,
        save_default_processing_job, save_output_deployment, save_user, save_workflow, saved_job_fits_workflow,
    },
};
use eyre::{Result, eyre};
use redact::Secret;
use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
    net::Ipv4Addr,
    path::PathBuf,
    process::{Command, ExitCode},
    time::Duration,
};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt};
use url::Url;

// CLI Styling
const STYLES: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::BrightWhite.on_default())
    .usage(styling::AnsiColor::BrightWhite.on_default())
    .literal(styling::AnsiColor::Green.on_default())
    .placeholder(styling::AnsiColor::Cyan.on_default());
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
        /// Elastic Upload Service ID or URL for sending after collection
        #[arg(
            help = "Elastic Upload Service ID or URL for sending after collection",
            long = "send"
        )]
        send_id: Option<String>,
        /// Save the effective invocation as a named job before continuing execution
        #[cfg(feature = "keystore")]
        #[arg(long = "save-job", value_name = "NAME")]
        save_job: Option<String>,
    },
    /// Start a web server to receive diagnostic bundle submissions
    #[cfg(feature = "server")]
    Serve {
        /// IPv4 address to bind the server to
        #[arg(
            long,
            long_help = "IPv4 address to bind the server to. Defaults to ESDIAG_BIND, then 127.0.0.1."
        )]
        bind: Option<Ipv4Addr>,
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
        /// Header that supplies the authenticated user identity
        #[arg(
            long,
            value_name = "HEADER",
            help = "Identity header for authenticated service requests"
        )]
        identity_header: Option<String>,
        /// Optional comma-separated web feature allowlist (advanced, job-builder)
        #[arg(long, value_name = "FEATURES")]
        web_features: Option<String>,
        /// Open the user-mode web onboarding flow on startup
        #[arg(long, hide = true)]
        onboarding: bool,
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
    #[command(
        after_help = "Commands: up, down, restart, status, logs, setup, open, auth, secrets, reset.\nRun esdiag local help for usage."
    )]
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
        #[arg(
            help = "Source to read diagnostic data from (archive, directory, known host, or Elastic Upload Service URL)"
        )]
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
        /// Force scrubbed archive mode on/off; when omitted auto-detects by filename containing 'scrubbed'
        #[arg(long)]
        scrubbed: Option<bool>,
        /// Save the effective invocation as a named job before continuing execution
        #[cfg(feature = "keystore")]
        #[arg(long = "save-job", value_name = "NAME")]
        save_job: Option<String>,
    },
    /// Send a raw diagnostic archive to Elastic Upload Service
    Send {
        /// Local diagnostic archive to send
        #[arg(help = "Local diagnostic archive file path")]
        file_name: String,
        /// Elastic Upload Service ID or URL
        #[arg(help = "Elastic Upload Service ID or URL")]
        upload_id: String,
        /// Elastic Upload Service API base URL
        #[arg(
            long,
            default_value = elastic_upload_service::DEFAULT_UPLOAD_API_URL,
            help = "Elastic Upload Service base URL"
        )]
        api_url: String,
    },
    /// Configure external systems for ESDiag: Elastic Stack assets, agent skills, and shell completions
    #[command(subcommand_required = true, arg_required_else_help = true)]
    Setup {
        #[command(subcommand)]
        command: SetupCommands,
    },
    #[cfg(feature = "agent")]
    /// Ask Kibana Agent Builder about diagnostics
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
    /// Manage the named Elastic CLI output context
    Output {
        #[command(subcommand)]
        command: OutputCommands,
    },
    /// Print the ESDiag version
    Version,
}

#[derive(Debug, Subcommand)]
enum OutputCommands {
    /// Validate and persist a named Elastic CLI context as the default output
    Set {
        /// Elastic CLI context name
        context: String,
    },
    /// Show the configured Elastic CLI output context
    Show,
    /// Clear the configured Elastic CLI output context
    Clear,
}

#[derive(Debug, Subcommand)]
enum SetupCommands {
    /// Install Elasticsearch assets into a saved Elasticsearch host
    #[cfg(feature = "setup")]
    Elasticsearch {
        /// Saved Elasticsearch host; defaults to the persisted output
        host: Option<String>,
    },
    /// Install Kibana assets into a saved Kibana host
    #[cfg(feature = "setup")]
    Kibana {
        /// Saved Kibana host; defaults to the persisted output's viewer
        host: Option<String>,
    },
    /// Install assets into a saved output and its linked Kibana viewer
    #[cfg(feature = "setup")]
    Output {
        /// Saved output host with role `send`; defaults to the persisted output
        name: Option<String>,
        #[command(flatten)]
        trial: TrialArgs,
    },
    /// Install assets into the deployment declared by ESDIAG_OUTPUT_* and ESDIAG_KIBANA_* variables
    #[cfg(feature = "setup")]
    Env {
        /// Read the variables from this file; the process environment takes precedence
        #[arg(long, value_name = "PATH")]
        env_file: Option<PathBuf>,
        #[command(flatten)]
        trial: TrialArgs,
    },
    /// Install assets into the services of an Elastic CLI (.elasticrc.yml) context
    #[cfg(feature = "setup")]
    Context {
        /// Context name; defaults to the current context
        name: Option<String>,
        /// Elastic CLI configuration file; defaults to ELASTIC_CLI_CONFIG_FILE, then home discovery
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
        #[command(flatten)]
        trial: TrialArgs,
    },
    /// Install the embedded ESDiag skill into coding agents
    #[cfg(feature = "agent")]
    Skills {
        /// Coding agents to install into; defaults to every detected agent
        #[arg(value_enum)]
        agents: Vec<AgentSkillTarget>,
        /// Replace locally modified or unrecognized ESDiag skill directories
        #[arg(long)]
        force: bool,
    },
    /// Install a shell completion script without editing shell rc files
    Completions {
        /// Shell to install completions for
        #[arg(value_enum)]
        shell: esdiag::setup::completions::CompletionShell,
        /// Print the script to stdout instead of installing it
        #[arg(long)]
        print: bool,
    },
}

#[cfg(feature = "setup")]
#[derive(Debug, clap::Args)]
struct TrialArgs {
    /// Start an Elasticsearch trial license when Agent Builder is not licensed (or set ESDIAG_SETUP_ACTIVATE_TRIAL=true)
    #[arg(long)]
    activate_trial: bool,
}

#[cfg(feature = "agent")]
#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// Submit one prompt to the configured Kibana Agent Builder agent
    Ask {
        /// Prompt passed unchanged to Agent Builder
        prompt: String,
        /// Agent Builder agent identifier
        #[arg(long = "agent", default_value = operations::agent::DEFAULT_AGENT_BUILDER_AGENT)]
        agent_id: String,
        /// Continue this Kibana Agent Builder conversation
        #[arg(long, conflicts_with = "new")]
        conversation: Option<String>,
        /// Explicitly start a new Kibana Agent Builder conversation
        #[arg(long, conflicts_with = "conversation")]
        new: bool,
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
    let Some(cli) = parse_cli()? else {
        return Ok(ExitCode::SUCCESS);
    };
    init_tracing(resolve_tracing_filter(&cli), resolve_log_file_filter(&cli));
    let stdout_owned = command_owns_stdout(&cli);
    let no_command = cli.command.is_none();
    if !no_command && !logs_to_terminal(&cli) {
        esdiag::data::enable_keystore_password_prompt();
    }

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        tracing::debug!("{:?}", panic);
        tracing::error!("{}", panic);
    }));

    clear_last_run_files()?;

    let format = cli.format;
    let setup_command = matches!(cli.command, Some(Commands::Setup { .. }));
    match run(cli, format).await {
        Ok(result) => {
            if let CommandOutcome::Completed(outcome) = result {
                write_terminal_outcome(format, &outcome)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            tracing::error!("{}", e);
            if !stdout_owned && !no_command {
                let failure = command_failure(&e, setup_command);
                write_terminal_outcome(format, &failure)?;
                return Ok(ExitCode::FAILURE);
            }
            Err(eyre!(e))
        }
    }
}

fn parse_cli() -> Result<Option<Cli>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let extension_profile = args
        .first()
        .is_some_and(|name| esdiag::env::is_elastic_cli_executable(name))
        || esdiag::env::is_elastic_cli_invocation();
    let mut command = cli_command(extension_profile);
    let parsed = if extension_profile && let Some(command_name) = unsupported_extension_command(&args) {
        Err(clap::Error::raw(
            ErrorKind::InvalidSubcommand,
            format!(
                "'{command_name}' is available through standalone 'esdiag' and is not part of the 'elastic diag' profile"
            ),
        ))
    } else if extension_profile && missing_extension_process_input(&args) {
        Err(clap::Error::raw(
            ErrorKind::MissingRequiredArgument,
            "elastic diag process requires an explicit application-bearing input such as '.es', '.kb', or '.context.es'",
        ))
    } else {
        command
            .clone()
            .try_get_matches_from(args)
            .and_then(|matches| Cli::from_arg_matches(&matches))
            .and_then(|cli| validate_extension_profile(cli, extension_profile))
    };

    match parsed {
        Ok(cli) if extension_profile && cli.command.is_none() => {
            command.print_help()?;
            println!("\n{}", elastic_cli_help_text());
            Ok(None)
        }
        Ok(cli) => Ok(Some(cli)),
        Err(err) if err.kind() == ErrorKind::DisplayHelp => {
            err.print()?;
            if extension_profile {
                println!("\n{}", elastic_cli_help_text());
            }
            Ok(None)
        }
        Err(err) if err.kind() == ErrorKind::DisplayVersion => {
            err.print()?;
            Ok(None)
        }
        Err(err) => err.exit(),
    }
}

fn cli_command(extension_profile: bool) -> clap::Command {
    let command = Cli::command();
    if !extension_profile {
        return command;
    }

    let mut command = command
        .bin_name("elastic diag")
        .mut_arg("agent", |argument| argument.hide(true))
        .mut_subcommand("host", |subcommand| subcommand.hide(true))
        .mut_subcommand("init", |subcommand| subcommand.hide(true))
        .mut_subcommand("local", |subcommand| subcommand.hide(true))
        .mut_subcommand("keystore", |subcommand| subcommand.hide(true));
    #[cfg(feature = "server")]
    {
        command = command.mut_subcommand("serve", |subcommand| subcommand.hide(true));
    }
    #[cfg(feature = "agent")]
    {
        command = command.mut_subcommand("agent", |subcommand| subcommand.hide(true));
    }
    command
}

fn validate_extension_profile(cli: Cli, extension_profile: bool) -> Result<Cli, clap::Error> {
    if !extension_profile {
        return Ok(cli);
    }
    let supported_command = match &cli.command {
        None
        | Some(Commands::Collect { .. })
        | Some(Commands::Process { .. })
        | Some(Commands::Send { .. })
        | Some(Commands::Output { .. })
        | Some(Commands::Version) => true,
        #[cfg(feature = "setup")]
        Some(Commands::Setup { .. }) => true,
        #[cfg(feature = "keystore")]
        Some(Commands::Job { .. }) => true,
        _ => false,
    };
    let supported = !cli.agent && supported_command;
    if supported {
        Ok(cli)
    } else {
        Err(clap::Error::raw(
            ErrorKind::InvalidSubcommand,
            "This command is available through standalone 'esdiag' and is not part of the 'elastic diag' profile",
        ))
    }
}

fn unsupported_extension_command(args: &[OsString]) -> Option<String> {
    let mut positional = Vec::new();
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let argument = argument.to_string_lossy();
        if argument == "--format" {
            let _ = arguments.next();
            continue;
        }
        if argument.starts_with("--format=") || argument.starts_with('-') {
            continue;
        }
        positional.push(argument.into_owned());
        if positional.len() == 2 || positional.first().is_some_and(|command| command != "help") {
            break;
        }
    }
    let command = match positional.as_slice() {
        [command] => command,
        [help, command] if help == "help" => command,
        _ => return None,
    };
    matches!(
        command.as_str(),
        "agent" | "host" | "init" | "keystore" | "local" | "secret" | "serve"
    )
    .then(|| command.clone())
}

fn missing_extension_process_input(args: &[OsString]) -> bool {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--format" {
            let _ = arguments.next();
            continue;
        }
        let argument = argument.to_string_lossy();
        if argument.starts_with('-') {
            continue;
        }
        return argument == "process" && arguments.next().is_none();
    }
    false
}

fn elastic_cli_help_text() -> &'static str {
    r#"Elastic CLI extension commands:
  collect, process, send, setup, job, output, help, version

Elastic CLI extension examples:
  elastic diag collect .es ./out
  elastic diag process .prod.es .diag.es
  elastic diag output set monitoring
  elastic diag send diagnostic.zip <upload-id>

Elastic CLI target references:
  .es, .elasticsearch  Use the active Elasticsearch context
  .kb, .kibana         Use the active Kibana context
  .context.service     Use a named application context
  .context.cloud/<deployment-id>/es
                       Use an Elasticsearch Cloud admin proxy target

Named contexts are read from .elasticrc, .elasticrc.json, .elasticrc.yaml,
.elasticrc.yml, or ELASTIC_CLI_CONFIG_FILE."#
}

fn init_tracing(terminal_filter: EnvFilter, file_filter: Option<EnvFilter>) {
    // Bridge `log` records from dependencies when available, but tolerate hosts that
    // already installed a global logger before invoking this binary.
    if let Err(err) = tracing_log::LogTracer::init() {
        eprintln!("tracing log bridge already initialized: {err}");
    }

    let terminal = fmt::layer()
        .with_ansi(std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none())
        .with_writer(std::io::stderr)
        .with_filter(terminal_filter);
    let file = file_filter.and_then(|filter| match esdiag::data::open_run_log(true) {
        Ok(file) => Some(
            fmt::layer()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file))
                .with_filter(filter),
        ),
        Err(err) => {
            eprintln!("Could not open the run log: {err}");
            None
        }
    });
    let subscriber = tracing_subscriber::registry().with(terminal).with(file);
    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("tracing subscriber already initialized: {err}");
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
        }) if operations::job::saved_job_writes_stdout(name).unwrap_or(false)
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
    let completions_script_stdout = matches!(
        &cli.command,
        Some(Commands::Setup {
            command: SetupCommands::Completions { print: true, .. }
        })
    );
    let version_stdout = matches!(&cli.command, Some(Commands::Version));
    direct_process_stream || saved_job_stream || local_raw_stdout || completions_script_stdout || version_stdout
}

fn resolve_tracing_filter(cli: &Cli) -> EnvFilter {
    if cli.debug {
        EnvFilter::new("debug")
    } else if is_agent_mode(cli) {
        EnvFilter::new("warn")
    } else if let Ok(filter) = EnvFilter::try_from_env("LOG_LEVEL") {
        filter
    } else if logs_to_terminal(cli) {
        EnvFilter::new(LOG_LEVEL)
    } else {
        EnvFilter::new("warn")
    }
}

fn resolve_log_file_filter(cli: &Cli) -> Option<EnvFilter> {
    if logs_to_terminal(cli) {
        None
    } else if cli.debug {
        Some(EnvFilter::new("debug"))
    } else {
        Some(EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new(LOG_LEVEL)))
    }
}

/// Long-running services log to stderr, which their supervisor captures.
fn logs_to_terminal(cli: &Cli) -> bool {
    #[cfg(feature = "server")]
    {
        matches!(cli.command, Some(Commands::Serve { .. }))
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = cli;
        false
    }
}

async fn run_local_lifecycle(args: Vec<OsString>) -> Result<CommandOutcome> {
    let raw_stdout = args
        .first()
        .and_then(|argument| argument.to_str())
        .map(|command| matches!(command, "help" | "--help" | "-h" | "version" | "--version" | "secrets"))
        .unwrap_or(true);
    let outcome = local::run(&args).await?;
    Ok(if raw_stdout {
        CommandOutcome::Streamed
    } else {
        CommandOutcome::Completed(outcome)
    })
}

#[tracing::instrument(skip_all)]
async fn run(cli: Cli, format: OutputFormat) -> Result<CommandOutcome> {
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
                identity_header,
                web_features,
                onboarding,
                kibana,
            } => {
                tracing::info!("Starting ESDiag server");
                let runtime_mode = resolve_serve_runtime_mode(mode)?;
                let bind = resolve_serve_bind(bind)?;
                let explicit_output = output.is_some();
                let onboarding_output_configured =
                    inspect_onboarding().is_ok_and(|readiness| readiness.output_configured);
                let (exporter, output_status) = match resolve_serve_exporter(output) {
                    Ok(exporter) => (
                        exporter,
                        if explicit_output || onboarding_output_configured {
                            "configured"
                        } else {
                            "unconfigured"
                        },
                    ),
                    Err(err)
                        if runtime_mode == RuntimeMode::User
                            && !explicit_output
                            && inspect_onboarding()
                                .is_ok_and(|readiness| !readiness.is_complete() || !readiness.output_configured) =>
                    {
                        tracing::info!("Starting user-mode web onboarding without a ready output: {err}");
                        (
                            onboarding_exporter()?,
                            if onboarding_output_configured {
                                "configured"
                            } else {
                                "unconfigured"
                            },
                        )
                    }
                    Err(err) => return Err(err),
                };
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
                        identity_header: identity_header.as_deref(),
                        web_features: web_features.as_deref(),
                        onboarding,
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
                            output: output_status.to_string(),
                        },
                    )?;
                }
                wait_for_shutdown_signal().await?;

                server.shutdown().await;
                Ok(CommandOutcome::Streamed)
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
                send_id,
                #[cfg(feature = "keystore")]
                save_job,
            } => {
                operations::collect::collect(operations::collect::CollectInput {
                    host,
                    output,
                    diagnostic_type: r#type,
                    include,
                    exclude,
                    sources,
                    account,
                    case,
                    opportunity,
                    user,
                    send_id,
                    #[cfg(feature = "keystore")]
                    save_job,
                })
                .await
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
                    operations::agent::agent_ask(
                        operations::agent::AgentAskInput {
                            prompt,
                            agent_id,
                            conversation,
                        },
                        render_agent_progress,
                    )
                    .await
                }
            },
            Commands::Host { command } => match command {
                HostCommands::Add {
                    name,
                    app,
                    target,
                    url_template,
                    args,
                } => {
                    operations::host::host_add(operations::host::HostAddInput {
                        name,
                        app,
                        target,
                        url_template,
                        update: args.into(),
                        keystore_password: get_password_for_secret_commands,
                    })
                    .await
                }
                HostCommands::Update { name, args } => {
                    operations::host::host_update(operations::host::HostUpdateInput {
                        name,
                        update: args.into(),
                        keystore_password: get_password_for_secret_commands,
                    })
                    .await
                }
                HostCommands::Remove { name } => operations::host::host_remove(name),
                HostCommands::List => operations::host::host_list(),
                HostCommands::Auth { target } => operations::host::host_auth(target).await,
                HostCommands::Legacy(args) => Err(legacy_host_command_error(&args)),
            },
            Commands::Keystore { command } => match command {
                KeystoreCommands::Add {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => operations::keystore::keystore_add(resolve_keystore_secret_input(
                    secret_id, username, password, apikey,
                )?),
                KeystoreCommands::Update {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => operations::keystore::keystore_update(resolve_keystore_secret_input(
                    secret_id, username, password, apikey,
                )?),
                KeystoreCommands::Remove {
                    secret_id,
                    username,
                    password,
                    apikey,
                } => operations::keystore::keystore_remove(operations::keystore::KeystoreRemoveInput {
                    keystore_password: get_password_for_secret_commands()?,
                    secret_id,
                    username,
                    password: password.map(Secret::new),
                    apikey: apikey.map(Secret::new),
                }),
                KeystoreCommands::Unlock { ttl } => {
                    let ttl = ttl
                        .as_deref()
                        .map(parse_unlock_ttl)
                        .transpose()?
                        .unwrap_or_else(default_unlock_ttl);
                    let (keystore_password, create_if_missing) = resolve_unlock_password()?;
                    operations::keystore::keystore_unlock(operations::keystore::KeystoreUnlockInput {
                        keystore_password,
                        ttl,
                        create_if_missing,
                    })
                }
                KeystoreCommands::Lock => operations::keystore::keystore_lock(),
                KeystoreCommands::Status => operations::keystore::keystore_status(),
                KeystoreCommands::Password => {
                    if !keystore_exists()? {
                        return Err(eyre!("No keystore exists to update the password."));
                    }
                    let current_password = get_password_for_secret_commands()?;
                    validate_existing_keystore_password(current_password.expose_secret())?;
                    let new_password = prompt_new_keystore_password()?;
                    let path =
                        rotate_keystore_password(current_password.expose_secret(), new_password.expose_secret())?;
                    tracing::info!("Keystore password updated for {path}");
                    Ok(CommandOutcome::Completed(CliOutcome::KeystoreChanged {
                        operation: KeystoreOperation::PasswordChanged,
                        secret_id: None,
                        path: Some(path),
                    }))
                }
                KeystoreCommands::Migrate => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let (migrated, unchanged) =
                        KnownHost::migrate_hosts_to_keystore(keystore_password.expose_secret())?;
                    tracing::info!(
                        "Keystore migration complete: migrated {migrated} host(s), unchanged {unchanged} host(s)."
                    );
                    Ok(CommandOutcome::Completed(CliOutcome::KeystoreChanged {
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
                scrubbed,
                #[cfg(feature = "keystore")]
                save_job,
            } => {
                let input = operations::process::ProcessInput {
                    input,
                    output,
                    #[cfg(feature = "agent")]
                    ask,
                    account,
                    case,
                    opportunity,
                    user,
                    sources,
                    scrubbed,
                    #[cfg(feature = "keystore")]
                    save_job,
                };
                #[cfg(feature = "agent")]
                {
                    operations::process::process_with_agent_progress(input, render_agent_progress).await
                }
                #[cfg(not(feature = "agent"))]
                {
                    operations::process::process(input).await
                }
            }
            Commands::Send {
                file_name,
                upload_id,
                api_url,
            } => {
                operations::send::send(operations::send::SendInput {
                    file_name,
                    upload_id,
                    api_url,
                })
                .await
            }
            Commands::Setup { command } => run_setup(command).await,
            Commands::Output { command } => match command {
                OutputCommands::Set { context } => operations::output::output_set(context),
                OutputCommands::Show => operations::output::output_show(),
                OutputCommands::Clear => operations::output::output_clear(),
            },
            Commands::Version => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                Ok(CommandOutcome::Streamed)
            }
            #[cfg(feature = "keystore")]
            Commands::Job { command } => match command {
                JobCommands::List => operations::job::job_list(),
                JobCommands::Run { name } => operations::job::job_run(name).await,
                JobCommands::Delete { name } => operations::job::job_delete(name),
            },
        }
    } else {
        #[cfg(all(feature = "server", feature = "desktop"))]
        {
            esdiag_gui::desktop::run(tauri::generate_context!());
            Ok(CommandOutcome::Streamed)
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
        println!("Will you collect new diagnostics, process existing diagnostics, or both?");
        println!("  1. Only collect new diagnostics");
        println!("  2. Process existing diagnostics");
        println!("  3. Both new and existing diagnostics");
        if let Some(workflow) = parse_onboarding_workflow(&prompt_with_default("Selection", "3")?) {
            return Ok(workflow);
        }
        println!("Choose 1, 2, or 3.");
    }
}

fn parse_onboarding_workflow(selection: &str) -> Option<OnboardingWorkflow> {
    match selection.trim().to_ascii_lowercase().as_str() {
        "1" | "collect" | "collecting" | "collect-only" => Some(OnboardingWorkflow::CollectOnly),
        "2" | "process" | "processing" | "existing" | "process-existing" => Some(OnboardingWorkflow::ProcessExisting),
        "3" | "both" | "collect-and-process" => Some(OnboardingWorkflow::CollectAndProcess),
        _ => None,
    }
}

fn prompt_output_location(local_available: bool) -> Result<OutputLocation> {
    loop {
        println!("Will processed diagnostics be stored locally or remotely?");
        println!("  1. Local ESDiag deployment");
        println!("  2. Remote Elasticsearch and Kibana deployment");
        let default = if local_available { "1" } else { "2" };
        match prompt_with_default("Selection", default)?.to_ascii_lowercase().as_str() {
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

fn parse_confirmation(answer: &str, default: bool) -> Option<bool> {
    match answer.trim().to_ascii_lowercase().as_str() {
        "" => Some(default),
        "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

fn prompt_confirmation(message: &str, default: bool) -> Result<bool> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Ok(false);
    }
    loop {
        print!("{message}");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Err(eyre!("Input closed; run `esdiag init` again to resume."));
        }
        if let Some(answer) = parse_confirmation(&line, default) {
            return Ok(answer);
        }
        println!("Enter y or n, or press Enter to accept the default.");
    }
}

fn prompt_confirm(message: &str) -> Result<bool> {
    prompt_confirmation(message, false)
}

fn prompt_confirm_default_yes(message: &str) -> Result<bool> {
    prompt_confirmation(message, true)
}

fn prompt_new_keystore_password() -> Result<Secret<String>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!("A new keystore password requires an interactive terminal."));
    }
    prompt_new_password_with(|prompt| Ok(Secret::new(rpassword::prompt_password(prompt)?)))
}

fn prompt_new_password_with(mut prompt: impl FnMut(&str) -> Result<Secret<String>>) -> Result<Secret<String>> {
    loop {
        let password = prompt("Enter new keystore password: ")?;
        if password.expose_secret().is_empty() {
            eprintln!("Keystore password cannot be empty. Try again.");
            continue;
        }
        if prompt("Confirm new keystore password: ")? == password {
            return Ok(password);
        }
        eprintln!("Passwords did not match. Try again.");
    }
}

fn unlock_keystore(ttl: Duration) -> Result<std::path::PathBuf> {
    let (keystore_password, create_if_missing) = resolve_unlock_password()?;
    operations::keystore::unlock_lease(&keystore_password, ttl, create_if_missing)
}

/// The password for an unlock, and whether it creates a new keystore after the user confirmed one.
fn resolve_unlock_password() -> Result<(Secret<String>, bool)> {
    if keystore_exists()? {
        return Ok((get_password_for_secret_commands()?, false));
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!(
            "No keystore exists and no interactive terminal is available to create one."
        ));
    }
    if !prompt_confirm("No keystore exists. Create one now? [y/N]: ")? {
        return Err(eyre!("Keystore unlock cancelled."));
    }
    Ok((prompt_new_keystore_password()?, true))
}

fn resolve_keystore_secret_input(
    secret_id: String,
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
) -> Result<operations::keystore::KeystoreSecretInput> {
    let keystore_password = get_password_for_secret_commands()?;
    let (username, password, apikey) = resolve_secret_input(username, password, apikey)?;
    Ok(operations::keystore::KeystoreSecretInput {
        secret_id,
        username,
        password: password.map(Secret::new),
        apikey: apikey.map(Secret::new),
        keystore_password,
    })
}

async fn run_init_wizard() -> Result<CommandOutcome> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(eyre!("`esdiag init` requires an interactive controlling terminal."));
    }

    let initial = inspect_onboarding()?;
    println!("ESDiag first-run initialization");
    if initial.is_complete() && !prompt_confirm("A complete configuration already exists. Replace values? [y/N]: ")? {
        return Ok(CommandOutcome::Completed(initialization_outcome(
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
            "Diagnostic user, preferably your email address",
            &default_diagnostic_user(),
        )?)?;
    }

    #[cfg(feature = "server")]
    if !initial.is_complete()
        && !inspect_onboarding()?.is_complete()
        && prompt_confirm("Continue setup in the web interface? [y/N]: ")?
    {
        return run_gui_onboarding().await;
    }
    let mut output_name_for_defaults = esdiag::data::ApplicationConfig::load()?.output.default;
    let mut output_url_for_defaults = output_name_for_defaults
        .as_ref()
        .and_then(KnownHost::get_known)
        .and_then(|host| host.concrete_url().map(Url::to_string));
    let mut most_recent_collect_host = None;
    let mut started_local_stack = false;

    let workflow = match config.workflow {
        Some(workflow) if prompt_confirm_default_yes(&format!("Resume workflow: {} [Y/n]: ", workflow.as_str()))? => {
            workflow
        }
        _ => prompt_onboarding_workflow()?,
    };
    println!("Selected workflow: {}", workflow.as_str());
    if let Some(previous) = config.workflow
        && previous != workflow
    {
        println!("Workflow changed from {} to {}.", previous.as_str(), workflow.as_str());
    }
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
            let (output_name, output_url, viewer_url, viewer_name, secret_id, auth, output_client, viewer_client) = loop {
                let (output_name, output_url, viewer_url, viewer_api_url, secret_id, auth) =
                    match prompt_output_location(
                        detected_esdiag_local_preset().is_some() || local::detected_runtime().is_some(),
                    )? {
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
                            let start_core = should_start_local_core_stack(
                                existing.is_none(),
                                runtime_available,
                                || {
                                    prompt_confirm_default_yes(
                                        "No local ESDiag deployment was detected. Start a local core stack now? [Y/n]: ",
                                    )
                                },
                            )?;
                            let local_preset = match existing {
                                Some(preset) => Some(preset),
                                None if start_core => {
                                    run_local_lifecycle(local_core_stack_start_args()).await?;
                                    println!("Local ESDiag stack is ready.");
                                    started_local_stack = true;
                                    detected_esdiag_local_preset()
                                }
                                None => None,
                            };
                            match local_preset {
                                Some(preset) => local_output(preset)?,
                                None => {
                                    if !prompt_confirm_default_yes(
                                        "Configure a remote output deployment instead? [Y/n]: ",
                                    )? {
                                        return Err(eyre!(
                                            "Local stack startup was declined. Select local output again to start it, or configure a remote output."
                                        ));
                                    }
                                    let output_name = prompt_with_default("Remote output host name", "diagnostics")?;
                                    let output_url = prompt_url_with_default(
                                        "Remote output Elasticsearch URL",
                                        "https://localhost:9200",
                                    )?;
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
                            let viewer_url =
                                prompt_url_with_default("Remote output Kibana URL", "https://localhost:5601")?;
                            let secret_id = prompt_with_default("Remote output credential name", &output_name)?;
                            let auth = prompt_api_key("Remote output", None)?;
                            (output_name, output_url, viewer_url.clone(), viewer_url, secret_id, auth)
                        }
                    };
                let viewer_name = format!("{output_name}-kb");
                let output_client = Client::try_from(Uri::try_from(esdiag::onboarding::output_validation_host(
                    output_url.clone(),
                    Application::Elasticsearch,
                    auth.clone(),
                )?)?)?;
                let viewer_client = Client::try_from(Uri::try_from(esdiag::onboarding::output_validation_host(
                    viewer_api_url,
                    Application::Kibana,
                    auth.clone(),
                )?)?)?;
                let output_result = validate_onboarding_endpoint(&output_client).await;
                let viewer_result = validate_onboarding_endpoint(&viewer_client).await;
                for (name, result) in [("Elasticsearch", &output_result), ("Kibana", &viewer_result)] {
                    if let Err(error) = result {
                        eprintln!("{name} output validation failed: {}.", endpoint_failure_reason(error));
                    }
                }
                if ensure_output_deployment_valid(output_result.is_ok(), viewer_result.is_ok()).is_err() {
                    eprintln!("Existing output configuration was not changed.");
                    if prompt_confirm_default_yes("Re-enter output endpoints and credentials? [Y/n]: ")? {
                        continue;
                    }
                    return Err(eyre!("Output validation failed. Run `esdiag init` again to resume."));
                }
                if !confirm_output_replacement(
                    &output_name,
                    &viewer_name,
                    &secret_id,
                    keystore_password.expose_secret(),
                )? {
                    println!("Existing output configuration was not changed. Choose an output deployment.");
                    continue;
                }
                break (
                    output_name,
                    output_url,
                    viewer_url,
                    viewer_name,
                    secret_id,
                    auth,
                    output_client,
                    viewer_client,
                );
            };
            output_name_for_defaults = Some(output_name.clone());
            output_url_for_defaults = Some(output_url.to_string());
            #[cfg(feature = "setup")]
            let viewer_space = esdiag::env::kibana_space_setting(operations::setup::process_kibana_space().as_deref());
            save_output_deployment(
                OutputDeploymentInput {
                    output_name,
                    output_url,
                    viewer_name,
                    viewer_url,
                    secret_id,
                    auth,
                },
                keystore_password.expose_secret(),
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
                let output_name = output_config
                    .output
                    .default
                    .clone()
                    .unwrap_or_else(|| "diagnostics".to_string());
                let activate_trial = setup::agent_builder_license_status(&output_client).await?
                    == setup::AgentBuilderLicense::Inactive
                    && prompt_confirm(&format!(
                        "Start a 30-day Elasticsearch trial license on {output_name} for Agent Builder? [y/N]: "
                    ))?;
                eprintln!("Installing ESDiag assets...");
                let targets = setup::pipeline::StackTargets {
                    elasticsearch: Some(output_client),
                    kibana: Some(setup::pipeline::KibanaTarget {
                        client: viewer_client,
                        space: viewer_space,
                    }),
                };
                let report = setup::pipeline::stack(&targets, activate_trial).await?.report;
                if report.is_complete() {
                    output_config.output.assets_version = Some(env!("CARGO_PKG_VERSION").to_string());
                } else {
                    let rerun = output_setup_rerun_command(&output_name, &report);
                    for warning in report
                        .failed_indices
                        .iter()
                        .map(|index| format!("Could not update {index}."))
                        .chain(report.warnings)
                    {
                        eprintln!("{warning}");
                    }
                    eprintln!("Rerun `{rerun}` to finish installing ESDiag assets.");
                }
            } else {
                eprintln!(
                    "ESDiag assets were not installed. Processing and Agent Builder are unavailable until you run `esdiag setup output {}`.",
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
                    .and_then(|host| host.secret_reference().map(str::to_string))
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
            if let Some(existing) = KnownHost::get_known(&name) {
                let add_role = match existing.concrete_url() {
                    Some(existing_url) if *existing_url == url => {
                        prompt_confirm_default_yes(&format!("Add the collect role to existing host '{name}'? [Y/n]: "))?
                    }
                    Some(existing_url) => prompt_confirm(&format!(
                        "Host '{name}' already exists at {existing_url}. Add the collect role and keep that URL? [y/N]: "
                    ))?,
                    None => prompt_confirm(&format!("Add the collect role to existing host '{name}'? [y/N]: "))?,
                };
                if !add_role {
                    return Err(eyre!("Collection host replacement was declined."));
                }
            }
            save_collect_host(
                CollectHostInput {
                    name: name.clone(),
                    app: Application::Elasticsearch,
                    url,
                    secret_id,
                    auth,
                },
                keystore_password
                    .as_ref()
                    .map(|password| password.expose_secret().as_str()),
            )?;
            most_recent_collect_host = Some(name);
        }
    }

    if workflow.collects_diagnostics() {
        let collect_host_default =
            most_recent_collect_host.ok_or_else(|| eyre!("Add a collect host before creating the default job"))?;
        let collect_host = prompt_collect_host(&collect_host_default)?;
        let collect_job_name = format!("{collect_host}-collect");
        let collect_job = esdiag::data::Job::builder()
            .collect_from(collect_host.clone())?
            .collect_to(format!("diagnostics/{collect_host}"))?;
        let collect_job_saved =
            confirm_default_job_replacement(&collect_job_name, None, !workflow.processes_diagnostics())?;
        if collect_job_saved {
            save_default_job(collect_job_name.clone(), collect_job)?;
        }
        if workflow.processes_diagnostics() && needs_processing_default_job(collect_job_saved)? {
            let output = esdiag::data::ApplicationConfig::load()?
                .output
                .default
                .ok_or_else(|| eyre!("A processing workflow requires an output deployment"))?;
            let process_job_name = format!("{collect_host}-process-{output}");
            if confirm_default_job_replacement(&process_job_name, Some(&collect_job_name), true)? {
                save_default_processing_job(process_job_name, collect_host)?;
            }
        }
    }

    let readiness = inspect_onboarding()?;
    if !readiness.is_complete() {
        return Err(eyre!("Initialization did not produce a complete reusable workflow."));
    }
    let outcome = initialization_outcome(initialization_skill_installation()?)?;
    if started_local_stack && prompt_confirm("Open the ESDiag web interface? [y/N]: ")? {
        run_local_lifecycle(vec![OsString::from("open")]).await?;
    }
    Ok(CommandOutcome::Completed(outcome))
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

fn endpoint_failure_reason(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("401") || error.contains("403") {
        "authentication or permission denied; check the API key and its privileges"
    } else if error.contains("certificate") || error.contains("tls") || error.contains("ssl") {
        "TLS verification failed; check the certificate and URL"
    } else if error.contains("dns") || error.contains("resolve") {
        "DNS lookup failed; check the hostname"
    } else if error.contains("timed out") || error.contains("timeout") {
        "connection timed out; check the hostname, port, and network access"
    } else if error.contains("connect") || error.contains("sending request") {
        "connection failed; check the hostname, port, and network access"
    } else {
        "unexpected response; check that the URL points to the correct Elastic application"
    }
}

async fn validate_onboarding_endpoint(client: &Client) -> std::result::Result<String, String> {
    tokio::time::timeout(Duration::from_secs(20), client.test_connection())
        .await
        .unwrap_or_else(|_| Err("connection timed out".to_string()))
}

fn prompt_collect_host(default: &str) -> Result<String> {
    let hosts = KnownHost::parse_hosts_yml()?;
    let names = hosts
        .iter()
        .filter(|(_, host)| host.has_role(HostRole::Collect) && !host.is_template())
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    println!("Saved collection hosts: {}", names.join(", "));
    loop {
        let name = prompt_with_default("Saved host name for the default job", default)?;
        if names.contains(&name.as_str()) {
            return Ok(name);
        }
        println!("Choose a saved collection host name from the list above. A URL cannot be used here.");
    }
}

fn confirm_output_replacement(
    output_name: &str,
    viewer_name: &str,
    secret_id: &str,
    keystore_password: &str,
) -> Result<bool> {
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
        return prompt_confirm(&format!("Replace {}? [y/N]: ", replaced.join(", ")));
    }
    Ok(true)
}

#[cfg(feature = "server")]
async fn run_gui_onboarding() -> Result<CommandOutcome> {
    let url = "http://127.0.0.1:2501/welcome";
    let address = "127.0.0.1:2501"
        .parse()
        .map_err(|err| eyre!("Invalid local onboarding address: {err}"))?;
    if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
        open_gui_onboarding_browser(url);
        println!("Using the ESDiag user-mode server already running at {url}. Press Ctrl+C to leave this terminal.");
        wait_for_shutdown_signal().await?;
        return Ok(CommandOutcome::Streamed);
    }

    let executable =
        std::env::current_exe().map_err(|err| eyre!("Unable to locate the running ESDiag binary: {err}"))?;
    let mut server = Command::new(executable)
        .args(["serve", "--mode", "user", "--onboarding", "--bind", "127.0.0.1"])
        .spawn()
        .map_err(|err| eyre!("Unable to start the ESDiag web server: {err}"))?;

    open_gui_onboarding_browser(url);
    let status = server
        .wait()
        .map_err(|err| eyre!("ESDiag web server stopped unexpectedly: {err}"))?;
    if !status.success() {
        return Err(eyre!("ESDiag web server exited with {status}"));
    }
    Ok(CommandOutcome::Streamed)
}

#[cfg(feature = "server")]
fn open_gui_onboarding_browser(url: &str) {
    if let Err(err) = esdiag::system_integration::open_browser(url) {
        tracing::warn!("ESDiag is running at {url}, but the browser could not be opened: {err}");
    }
}

#[cfg(feature = "server")]
fn onboarding_exporter() -> Result<Exporter> {
    let directory = std::env::temp_dir().join("esdiag-onboarding");
    std::fs::create_dir_all(&directory)?;
    Exporter::try_from(Uri::Directory(directory))
}

fn default_collect_host_name() -> Option<String> {
    KnownHost::parse_hosts_yml()
        .ok()?
        .into_iter()
        .find_map(|(name, host)| host.has_role(HostRole::Collect).then_some(name))
}

#[derive(Debug, PartialEq, Eq)]
struct ConfiguredDefaultJob {
    name: String,
    fits_workflow: bool,
}

/// The configured default job that initialization must ask before replacing.
/// A default chosen earlier in this run (`set_this_run`) may be replaced
/// without asking.
fn configured_default_job(set_this_run: Option<&str>) -> Result<Option<ConfiguredDefaultJob>> {
    let Some(name) = esdiag::data::ApplicationConfig::load()?.job.default else {
        return Ok(None);
    };
    if Some(name.as_str()) == set_this_run {
        return Ok(None);
    }
    Ok(Some(ConfiguredDefaultJob {
        name,
        fits_workflow: inspect_onboarding()?.default_job_configured,
    }))
}

/// Asks before overwriting the saved job `name` or the configured default job,
/// and returns whether to save `name` as the default. `set_this_run` is a
/// default job this initialization already chose, which may be replaced
/// without asking. `last_step` is false when a later step creates another
/// default job, so a kept saved job only has to fit the workflow at the end.
fn confirm_default_job_replacement(name: &str, set_this_run: Option<&str>, last_step: bool) -> Result<bool> {
    let replaces_job = esdiag::data::load_saved_jobs()?.contains_key(name);
    let current = configured_default_job(set_this_run)?;
    let replaces_default = current.as_ref().is_some_and(|current| current.name != name);
    if !(replaces_job || replaces_default) {
        return Ok(true);
    }
    let replaced = match (replaces_job, replaces_default) {
        (true, true) => format!("existing job '{name}' and configured default job"),
        (true, false) => format!("existing job '{name}'"),
        (false, true) => "the configured default job".to_string(),
        (false, false) => unreachable!(),
    };
    let misfit = match current.as_ref() {
        Some(current) => (!current.fits_workflow).then(|| ("default", current.name.clone())),
        None if last_step && !saved_job_fits_workflow(name)? => Some(("saved", name.to_string())),
        None => None,
    };
    if let Some((kind, job)) = misfit {
        if prompt_confirm_default_yes(&format!(
            "The {kind} job '{job}' does not fit the selected workflow. Replace {replaced}? [Y/n]: "
        ))? {
            return Ok(true);
        }
        return Err(eyre!(
            "Kept the {kind} job '{job}', which does not fit the selected workflow. Run `esdiag init` again to choose a compatible default job."
        ));
    }
    if prompt_confirm(&format!("Replace {replaced}? [y/N]: "))? {
        return Ok(true);
    }
    keep_default_job(name, current.map(|current| current.name).as_deref())?;
    Ok(false)
}

/// Leaves saved jobs unchanged after a declined replacement. Without a
/// configured `current` default to keep, the existing saved `name` becomes the
/// default.
fn keep_default_job(name: &str, current: Option<&str>) -> Result<()> {
    if let Some(current) = current {
        println!("Keeping existing default job '{current}'.");
        return Ok(());
    }
    let mut config = esdiag::data::ApplicationConfig::load()?;
    config.job.default = Some(name.to_string());
    config.save()?;
    println!("Keeping existing job '{name}' as the default job.");
    Ok(())
}

/// Whether collect-and-process initialization still needs to save a processing
/// default job after the collection step. A kept default that already fits the
/// workflow is left alone.
fn needs_processing_default_job(collect_job_saved: bool) -> Result<bool> {
    Ok(collect_job_saved || !inspect_onboarding()?.default_job_configured)
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
    let recovery_command = "esdiag setup skills [claude|codex|opencode]...";
    let detected_names = detected.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let prompt = if detected_names.is_empty() {
        "No supported coding-agent home was detected. Install the embedded ESDiag skill now? [y/N]: ".to_string()
    } else {
        format!("Detected coding agents: {detected_names}. Install the embedded ESDiag skill? [y/N]: ")
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
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Err(eyre!("Input closed; run `esdiag init` again to resume."));
    }
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
    loop {
        print!("{message}");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Err(eyre!("Input closed; run `esdiag init` again to resume."));
        }
        let value = line.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
        println!("A value is required. Please try again.");
    }
}

fn prompt_with_default(message: &str, default: &str) -> Result<String> {
    if default.is_empty() {
        print!("{message}: ");
    } else {
        print!("{message} [{default}]: ");
    }
    std::io::stdout().flush()?;
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Err(eyre!("Input closed; run `esdiag init` again to resume."));
    }
    let value = line.trim();
    if value.is_empty() && default.is_empty() {
        return prompt_required(&format!("{message}: "));
    }
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn prompt_url_with_default(message: &str, default: &str) -> Result<Url> {
    loop {
        let value = prompt_with_default(message, default)?;
        match Url::parse(&value) {
            Ok(url)
                if matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none() =>
            {
                return Ok(url);
            }
            _ => println!("Enter an HTTP or HTTPS URL without embedded credentials."),
        }
    }
}

#[derive(Clone)]
struct EsdiagLocalPreset {
    elasticsearch_url: String,
    kibana_url: String,
    kibana_api_url: String,
    apikey: Option<String>,
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
    Some(esdiag::env::user_home_dir().ok()?.join(".esdiag/local"))
}

fn prompt_api_key(label: &str, local_preset: Option<&EsdiagLocalPreset>) -> Result<SecretAuth> {
    let has_local_key = local_preset.and_then(|preset| preset.apikey.as_ref()).is_some();
    let paste_selection = if has_local_key { "4" } else { "3" };
    loop {
        println!("{label} API key source:");
        println!("  1. Read from a file");
        println!("  2. Read from an environment variable");
        if has_local_key {
            println!("  3. Read from detected esdiag-local configuration");
        }
        println!("  {paste_selection}. Paste securely");
        let source = prompt_with_default("Selection", "3")?;
        let apikey = match source.as_str() {
            "1" | "file" => prompt_required("API key file path: ").and_then(|path| read_api_key_file(&path)),
            "2" | "environment" | "env" => {
                let name = prompt_with_default("API key environment variable", "ESDIAG_OUTPUT_APIKEY")?;
                std::env::var(&name).map_err(|_| eyre!("{name} is not set"))
            }
            value if value == paste_selection || value == "paste" => {
                prompt_missing_secret_value(&format!("Enter {label} API key: "))
            }
            "3" | "esdiag-local" | "local" => local_preset
                .and_then(|preset| preset.apikey.clone())
                .ok_or_else(|| eyre!("No usable ESDiag local API key was detected")),
            _ => Err(eyre!("Choose one of the listed API key sources")),
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
struct SkillInstallationRun {
    detected: Vec<SkillTarget>,
    selected: Vec<SkillTarget>,
    version: String,
    digest: String,
    results: Vec<AgentSkillTargetResult>,
    failed: bool,
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
fn run_agent_skills(targets: Vec<AgentSkillTarget>, force: bool) -> Result<CommandOutcome> {
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
    Ok(CommandOutcome::Completed(CliOutcome::AgentSkills {
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
    default_diagnostic_user_from(
        std::env::var("EMAIL").ok(),
        || command_stdout("git", &["config", "--get", "user.email"]),
        || command_stdout("whoami", &[]),
    )
}

fn default_diagnostic_user_from(
    email: Option<String>,
    git_email: impl FnOnce() -> Option<String>,
    login: impl FnOnce() -> Option<String>,
) -> String {
    let is_email = |value: &String| value.contains('@');
    email
        .filter(is_email)
        .or_else(|| git_email().filter(is_email))
        .or_else(login)
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn command_stdout(program: &str, arguments: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| output.status.success() && !value.is_empty())
}

fn should_error_for_missing_subcommand(arg_count: usize, has_no_command: bool) -> bool {
    arg_count > 1 && has_no_command
}

fn clear_last_run_files() -> Result<()> {
    let last_run = esdiag::env::runtime_config_dir()?.join("last_run");
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
fn resolve_serve_bind(bind: Option<Ipv4Addr>) -> Result<Ipv4Addr> {
    if let Some(bind) = bind {
        return Ok(bind);
    }
    match std::env::var("ESDIAG_BIND") {
        Ok(value) => value
            .trim()
            .parse()
            .map_err(|err| eyre!("Invalid ESDIAG_BIND '{value}', expected an IPv4 address: {err}")),
        Err(std::env::VarError::NotPresent) => Ok(Ipv4Addr::LOCALHOST),
        Err(err) => Err(eyre!("Failed to read ESDIAG_BIND: {err}")),
    }
}

#[cfg(feature = "server")]
fn resolve_serve_exporter(output: Option<String>) -> Result<Exporter> {
    Exporter::try_from(Uri::try_from(output)?)
}

async fn run_setup(command: SetupCommands) -> Result<CommandOutcome> {
    match command {
        #[cfg(feature = "setup")]
        SetupCommands::Elasticsearch { host } => operations::setup::setup_elasticsearch(host).await,
        #[cfg(feature = "setup")]
        SetupCommands::Kibana { host } => operations::setup::setup_kibana(host).await,
        #[cfg(feature = "setup")]
        SetupCommands::Output { name, trial } => operations::setup::setup_output(name, trial.activate_trial).await,
        #[cfg(feature = "setup")]
        SetupCommands::Env { env_file, trial } => operations::setup::setup_env(env_file, trial.activate_trial).await,
        #[cfg(feature = "setup")]
        SetupCommands::Context { name, config, trial } => {
            operations::setup::setup_context(name, config, trial.activate_trial).await
        }
        #[cfg(feature = "agent")]
        SetupCommands::Skills { agents, force } => run_agent_skills(agents, force),
        SetupCommands::Completions { shell, print } => {
            use clap::CommandFactory;
            use esdiag::setup::completions::{install_completion, user_home, write_completion_script};

            let mut cmd = Cli::command();
            if print {
                let mut stdout = std::io::stdout().lock();
                write_completion_script(shell, &mut cmd, &mut stdout)?;
                return Ok(CommandOutcome::Streamed);
            }
            let home = user_home()?;
            let (path, instruction) = install_completion(shell, &mut cmd, &home)?;
            Ok(CommandOutcome::Completed(CliOutcome::CompletionInstalled {
                shell: shell.as_str().to_string(),
                path: path.display().to_string(),
                instruction,
            }))
        }
    }
}

/// The command that finishes an incomplete `esdiag init` asset setup, opting into the trial when the license blocked Kibana.
#[cfg(feature = "setup")]
fn output_setup_rerun_command(output_name: &str, report: &setup::SetupReport) -> String {
    let needs_trial = report
        .warnings
        .iter()
        .any(|warning| warning == setup::pipeline::TRIAL_OPT_IN_WARNING);
    let flag = if needs_trial { " --activate-trial" } else { "" };
    format!("esdiag setup output {output_name}{flag}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_password_prompt_retries_until_confirmed() {
        let mut answers = ["", "first", "typo", "second", "second"].into_iter();
        let mut prompts = Vec::new();

        let password = super::prompt_new_password_with(|prompt| {
            prompts.push(prompt.to_string());
            Ok(Secret::new(answers.next().expect("unexpected prompt").to_string()))
        })
        .expect("password");

        assert_eq!(password.expose_secret(), "second");
        assert_eq!(prompts.len(), 5);
        assert!(answers.next().is_none());
    }

    #[test]
    fn confirmations_require_yes_or_no() {
        assert_eq!(super::parse_confirmation("claude", false), None);
        assert_eq!(super::parse_confirmation("typo", true), None);
        assert_eq!(super::parse_confirmation("", false), Some(false));
        assert_eq!(super::parse_confirmation("", true), Some(true));
        assert_eq!(super::parse_confirmation(" YES ", false), Some(true));
        assert_eq!(super::parse_confirmation("No", true), Some(false));
    }

    #[test]
    fn onboarding_workflow_uses_one_three_option_selection() {
        use esdiag::data::OnboardingWorkflow;

        assert_eq!(
            super::parse_onboarding_workflow("1"),
            Some(OnboardingWorkflow::CollectOnly)
        );
        assert_eq!(
            super::parse_onboarding_workflow("2"),
            Some(OnboardingWorkflow::ProcessExisting)
        );
        assert_eq!(
            super::parse_onboarding_workflow("3"),
            Some(OnboardingWorkflow::CollectAndProcess)
        );
        assert_eq!(
            super::parse_onboarding_workflow("both"),
            Some(OnboardingWorkflow::CollectAndProcess)
        );
        assert_eq!(super::parse_onboarding_workflow("invalid"), None);
    }

    #[test]
    fn endpoint_errors_are_actionable_without_echoing_response_secrets() {
        for (error, expected) in [
            ("HTTP 401 secret-value", "authentication"),
            ("HTTP 403", "permission"),
            ("TLS certificate error", "TLS"),
            ("DNS lookup failed", "DNS"),
            ("connection timed out", "timed out"),
            ("connection refused", "connection failed"),
            ("bad body secret-value", "unexpected response"),
        ] {
            let message = super::endpoint_failure_reason(error);
            assert!(message.contains(expected));
            assert!(!message.contains("secret-value"));
        }
    }

    #[cfg(feature = "setup")]
    use super::TrialArgs;
    #[cfg(feature = "agent")]
    use super::{AgentCommands, AgentSkillTarget, install_skill_targets};
    use super::{
        Cli, Commands, ConfiguredDefaultJob, HostCommands, KeystoreCommands, SetupCommands, cli_command,
        command_owns_stdout, configured_default_job, default_diagnostic_user_from, detected_esdiag_local_preset,
        elastic_cli_help_text, ensure_output_deployment_valid, is_agent_mode, keep_default_job,
        local_core_stack_start_args, missing_extension_process_input, needs_processing_default_job,
        resolve_log_file_filter, resolve_secret_input_with_prompt, resolve_tracing_filter, saved_job_fits_workflow,
        should_error_for_missing_subcommand, should_run_output_setup, should_start_local_core_stack,
        validate_extension_profile,
    };
    #[cfg(feature = "server")]
    use super::{resolve_serve_bind, resolve_serve_exporter, resolve_serve_runtime_mode};
    use clap::{FromArgMatches, Parser, error::ErrorKind};
    use esdiag::cli_output::OutputFormat;
    use esdiag::data::HostRole;
    #[cfg(feature = "server")]
    use esdiag::server::RuntimeMode;
    #[cfg(feature = "agent")]
    use esdiag::setup::skills::{SkillEnvironment, SkillTarget};
    #[cfg(feature = "keystore")]
    use esdiag::{
        job::model::{ExportTarget, Input, Job, Process},
        processor::Identifiers,
    };
    use redact::Secret;
    #[cfg(feature = "server")]
    use std::net::Ipv4Addr;
    use std::{ffi::OsString, sync::Mutex};
    use tempfile::TempDir;

    /// Mirrors the library's `TestEnv` for this binary's test process, which
    /// links the library without its `cfg(test)` items and so cannot use that
    /// type directly.
    ///
    /// Serializes environment-mutating tests, captures every variable it
    /// changes, and restores it on drop so a panicking test cannot leak values
    /// into later tests. Poisoning is recovered rather than propagated;
    /// otherwise one failing test cascades into every later environment test.
    struct TestEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<OsString>)>,
        tmp: TempDir,
    }

    fn parse_extension(args: &[&str]) -> Result<Cli, clap::Error> {
        let matches = cli_command(true).try_get_matches_from(args)?;
        let cli = Cli::from_arg_matches(&matches)?;
        validate_extension_profile(cli, true)
    }

    impl TestEnv {
        fn new() -> Self {
            static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
            let guard = LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let tmp = TempDir::new().expect("temp dir");
            let config_dir = tmp.path().join(".esdiag");
            std::fs::create_dir_all(&config_dir).expect("create config dir");

            let mut env = Self {
                _guard: guard,
                previous: Vec::new(),
                tmp,
            };
            let home = env.tmp.path().to_path_buf();
            env.set("HOME", &home);
            env.set("USERPROFILE", &home);
            env.set("ESDIAG_HOSTS", config_dir.join("hosts.yml"));
            env.set("ESDIAG_KEYSTORE", config_dir.join("secrets.yml"));
            env.set("ESDIAG_SETTINGS", config_dir.join("settings.yml"));
            env.remove("ESDIAG_KEYSTORE_PASSWORD");
            env
        }

        fn set(&mut self, key: &'static str, value: impl AsRef<std::ffi::OsStr>) {
            self.capture(key);
            unsafe {
                std::env::set_var(key, value);
            }
        }

        fn remove(&mut self, key: &'static str) {
            self.capture(key);
            unsafe {
                std::env::remove_var(key);
            }
        }

        fn capture(&mut self, key: &'static str) {
            if self.previous.iter().any(|(existing, _)| *existing == key) {
                return;
            }
            self.previous.push((key, std::env::var_os(key)));
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            for (key, value) in self.previous.drain(..).rev() {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    /// A `TestEnv` with the keystore already unlocked.
    fn setup_env() -> TestEnv {
        let mut env = TestEnv::new();
        env.set("ESDIAG_KEYSTORE_PASSWORD", "pw");
        env
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
    }

    #[cfg(feature = "agent")]
    #[test]
    fn setup_skills_allows_repeatable_positional_agents() {
        let cli = Cli::parse_from(["esdiag", "setup", "skills", "claude", "opencode", "--force"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Setup {
                command: SetupCommands::Skills {
                    agents,
                    force: true,
                }
            }) if matches!(agents.as_slice(), [AgentSkillTarget::Claude, AgentSkillTarget::OpenCode])
        ));
    }

    #[test]
    fn setup_requires_a_subcommand() {
        let error = Cli::try_parse_from(["esdiag", "setup"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        assert_ne!(error.exit_code(), 0);
        let help = error.to_string();
        for subcommand in ["completions"] {
            assert!(help.contains(subcommand), "{help}");
        }
        #[cfg(feature = "setup")]
        for subcommand in ["elasticsearch", "kibana", "output", "env", "context"] {
            assert!(help.contains(subcommand), "{help}");
        }
        #[cfg(feature = "agent")]
        assert!(help.contains("skills"), "{help}");
    }

    #[test]
    fn removed_setup_spellings_are_usage_errors() {
        for args in [
            &["esdiag", "setup", "diagnostics-output"][..],
            &["esdiag", "agent", "skills"],
            &["esdiag", "completions", "zsh"],
            &["esdiag", "completions", "install", "zsh"],
            &["esdiag", "setup", "completions", "install", "zsh"],
        ] {
            assert!(Cli::try_parse_from(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn setup_completions_prints_only_with_the_flag() {
        let cli = Cli::parse_from(["esdiag", "setup", "completions", "zsh", "--print"]);
        assert!(command_owns_stdout(&cli));
        let cli = Cli::parse_from(["esdiag", "setup", "completions", "zsh"]);
        assert!(!command_owns_stdout(&cli));
    }

    #[test]
    fn generated_completions_cover_the_setup_tree() {
        use clap::CommandFactory;
        let mut script = Vec::new();
        esdiag::setup::completions::write_completion_script(
            esdiag::setup::completions::CompletionShell::Zsh,
            &mut Cli::command(),
            &mut script,
        )
        .unwrap();
        let script = String::from_utf8(script).unwrap();
        let root = Cli::command();
        let setup = root.find_subcommand("setup").expect("setup command");
        for subcommand in setup.get_subcommands().map(|command| command.get_name()) {
            assert!(script.contains(&format!("'{subcommand}:")), "{subcommand}");
        }
        assert!(root.find_subcommand("completions").is_none());
        #[cfg(feature = "agent")]
        assert!(
            root.find_subcommand("agent")
                .expect("agent command")
                .find_subcommand("skills")
                .is_none()
        );
    }

    #[cfg(feature = "setup")]
    #[test]
    fn setup_service_and_stack_forms_parse() {
        let parse = |args: &[&str]| Cli::try_parse_from(args).map(|cli| cli.command);
        assert!(matches!(
            parse(&["esdiag", "setup", "kibana", "kibana"]).unwrap(),
            Some(Commands::Setup { command: SetupCommands::Kibana { host: Some(host) } }) if host == "kibana"
        ));
        assert!(matches!(
            parse(&["esdiag", "setup", "elasticsearch"]).unwrap(),
            Some(Commands::Setup {
                command: SetupCommands::Elasticsearch { host: None }
            })
        ));
        assert!(matches!(
            parse(&["esdiag", "setup", "output", "prod", "--activate-trial"]).unwrap(),
            Some(Commands::Setup {
                command: SetupCommands::Output { name: Some(name), trial: TrialArgs { activate_trial: true } }
            }) if name == "prod"
        ));
        assert!(matches!(
            parse(&["esdiag", "setup", "env", "--env-file", "deploy.env"]).unwrap(),
            Some(Commands::Setup {
                command: SetupCommands::Env { env_file: Some(path), trial: TrialArgs { activate_trial: false } }
            }) if path == std::path::Path::new("deploy.env")
        ));
        assert!(matches!(
            parse(&["esdiag", "setup", "context", "prod", "--config", "rc.yml"]).unwrap(),
            Some(Commands::Setup {
                command: SetupCommands::Context {
                    name: Some(_),
                    config: Some(_),
                    ..
                }
            })
        ));
        for single in ["elasticsearch", "kibana"] {
            assert!(
                parse(&["esdiag", "setup", single, "--activate-trial"]).is_err(),
                "{single}"
            );
        }
    }

    #[cfg(feature = "setup")]
    #[test]
    fn init_rerun_hint_opts_into_the_trial_only_when_the_license_blocked_kibana() {
        let mut report = esdiag::setup::SetupReport::default();
        report.failed_indices.push("logs-esdiag".to_string());
        assert_eq!(
            super::output_setup_rerun_command("diagnostics", &report),
            "esdiag setup output diagnostics"
        );
        report
            .warnings
            .push(esdiag::setup::pipeline::TRIAL_OPT_IN_WARNING.to_string());
        assert_eq!(
            super::output_setup_rerun_command("diagnostics", &report),
            "esdiag setup output diagnostics --activate-trial"
        );
    }

    #[cfg(not(feature = "setup"))]
    #[test]
    fn stack_setup_subcommands_require_the_setup_feature() {
        assert!(Cli::try_parse_from(["esdiag", "setup", "completions", "bash", "--print"]).is_ok());
        for subcommand in ["elasticsearch", "kibana", "output", "env", "context"] {
            assert!(
                Cli::try_parse_from(["esdiag", "setup", subcommand]).is_err(),
                "{subcommand}"
            );
        }
    }

    #[cfg(not(feature = "agent"))]
    #[test]
    fn setup_skills_requires_the_agent_feature() {
        assert!(Cli::try_parse_from(["esdiag", "setup", "skills"]).is_err());
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

    #[test]
    fn default_diagnostic_user_prefers_email_then_git_then_login() {
        let some = |value: &str| Some(value.to_string());
        let unused = || -> Option<String> { panic!("later sources are not consulted") };

        assert_eq!(
            default_diagnostic_user_from(some("operator@example.com"), unused, unused),
            "operator@example.com"
        );
        assert_eq!(
            default_diagnostic_user_from(some("not-an-email"), || some("git@example.com"), unused),
            "git@example.com"
        );
        assert_eq!(
            default_diagnostic_user_from(None, || some("no-at-sign"), || some("shell-user\n")),
            "shell-user"
        );
        assert_eq!(default_diagnostic_user_from(None, || None, || None), "");
    }

    fn saved_default_job() -> Option<String> {
        esdiag::data::ApplicationConfig::load()
            .expect("load config")
            .job
            .default
    }

    /// Saves a collect host `src`, a collection-only job `src-collect`, and a
    /// processing job `src-process-out`, with `workflow` and `default` selected.
    fn configure_jobs(env: &TestEnv, workflow: esdiag::data::OnboardingWorkflow, default: Option<&str>) {
        let config_dir = env.tmp.path().join(".esdiag");
        std::fs::write(
            config_dir.join("hosts.yml"),
            "src:\n  app: elasticsearch\n  roles:\n    - collect\n  url: http://localhost:9200/\n",
        )
        .expect("write hosts");
        std::fs::write(
            config_dir.join("jobs.yml"),
            "src-collect:\n  collect:\n    host: src\n    diagnostic_type: support\n  action: collect\n  output_dir: /tmp/src-collect\n\
             src-process-out:\n  collect:\n    host: src\n    save_dir: /tmp/src-bundle\n  action: process\n  output:\n    type: known-host\n    name: out\n",
        )
        .expect("write jobs");
        let mut config = esdiag::data::ApplicationConfig::new();
        config.user = Some("tester@example.com".to_string());
        config.workflow = Some(workflow);
        config.output.default = Some("out".to_string());
        config.job.default = default.map(str::to_string);
        config.save().expect("save config");
    }

    #[test]
    fn a_default_job_set_this_run_is_replaced_without_asking() {
        let env = setup_env();
        configure_jobs(
            &env,
            esdiag::data::OnboardingWorkflow::CollectAndProcess,
            Some("src-collect"),
        );

        assert_eq!(
            configured_default_job(Some("src-collect")).expect("configured default"),
            None
        );
        assert_eq!(
            configured_default_job(None).expect("configured default"),
            Some(ConfiguredDefaultJob {
                name: "src-collect".to_string(),
                fits_workflow: false,
            })
        );
    }

    #[test]
    fn a_configured_default_job_reports_whether_it_fits_the_workflow() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectOnly, Some("src-collect"));
        assert!(
            configured_default_job(None)
                .expect("configured default")
                .is_some_and(|current| current.fits_workflow)
        );

        configure_jobs(
            &env,
            esdiag::data::OnboardingWorkflow::CollectAndProcess,
            Some("src-collect"),
        );
        assert!(
            configured_default_job(None)
                .expect("configured default")
                .is_some_and(|current| !current.fits_workflow)
        );
    }

    #[test]
    fn declining_replacement_keeps_the_configured_default_job() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectOnly, Some("old-job"));

        keep_default_job("src-collect", Some("old-job")).expect("keep default job");

        assert_eq!(saved_default_job().as_deref(), Some("old-job"));
    }

    #[test]
    fn declining_replacement_without_a_default_keeps_the_saved_job_as_default() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectOnly, None);

        keep_default_job("src-collect", None).expect("keep default job");

        assert_eq!(saved_default_job().as_deref(), Some("src-collect"));
    }

    #[test]
    fn a_kept_default_that_fits_the_workflow_skips_the_processing_job() {
        let env = setup_env();
        configure_jobs(
            &env,
            esdiag::data::OnboardingWorkflow::CollectAndProcess,
            Some("src-process-out"),
        );
        assert!(!needs_processing_default_job(false).expect("processing default"));
        assert!(needs_processing_default_job(true).expect("processing default"));
    }

    #[test]
    fn a_promoted_processing_job_is_not_replaced() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectAndProcess, None);

        keep_default_job("src-process-out", None).expect("keep default job");

        assert!(!needs_processing_default_job(false).expect("processing default"));
        assert_eq!(saved_default_job().as_deref(), Some("src-process-out"));
    }

    #[test]
    fn a_promoted_collection_job_still_needs_the_processing_job() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectAndProcess, None);

        keep_default_job("src-collect", None).expect("keep default job");

        assert!(needs_processing_default_job(false).expect("processing default"));
    }

    #[test]
    fn a_saved_job_fits_only_the_workflow_it_was_built_for() {
        let env = setup_env();
        configure_jobs(&env, esdiag::data::OnboardingWorkflow::CollectOnly, None);
        assert!(saved_job_fits_workflow("src-collect").expect("fits workflow"));
        assert!(!saved_job_fits_workflow("src-process-out").expect("fits workflow"));
        assert!(!saved_job_fits_workflow("missing").expect("fits workflow"));
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
        let _env = setup_env();
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
    fn elastic_cli_invocation_marker_requires_one() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_ELASTIC_CLI", "1");
        assert!(esdiag::env::is_elastic_cli_invocation());

        env.set("ESDIAG_ELASTIC_CLI", "true");
        assert!(!esdiag::env::is_elastic_cli_invocation());
    }

    #[test]
    fn elastic_cli_help_text_documents_target_references() {
        let help = elastic_cli_help_text();

        assert!(help.contains("elastic diag collect .es ./out"));
        assert!(help.contains(".context.service"));
        assert!(help.contains(".context.cloud/<deployment-id>/es"));
        assert!(!help.contains("\n  .cloud               "));
    }

    #[test]
    fn executable_name_selects_extension_profile() {
        assert!(esdiag::env::is_elastic_cli_executable(std::ffi::OsStr::new(
            "elastic-diag"
        )));
        assert!(esdiag::env::is_elastic_cli_executable(std::ffi::OsStr::new(
            "elastic-diag.exe"
        )));
        assert!(!esdiag::env::is_elastic_cli_executable(std::ffi::OsStr::new("esdiag")));
    }

    #[test]
    fn extension_help_hides_standalone_administration() {
        let mut command = cli_command(true);
        let help = command.render_help().to_string();

        assert!(help.contains("Usage: elastic diag"));
        for supported in ["collect", "process", "send", "output", "version"] {
            assert!(help.contains(supported), "{supported} must be visible");
        }
        for standalone in ["host", "keystore", "local", "init"] {
            assert!(
                !help.contains(&format!("  {standalone} ")),
                "{standalone} must be hidden"
            );
        }
    }

    #[test]
    fn extension_profile_accepts_send_and_rejects_host() {
        let send = parse_extension(&["elastic-diag", "send", "diagnostic.zip", "upload-id"]).expect("send must parse");
        assert!(matches!(send.command, Some(Commands::Send { .. })));
        assert!(
            cli_command(true)
                .try_get_matches_from(["elastic-diag", "upload", "diagnostic.zip", "upload-id"])
                .is_err(),
            "the extension must not accept the retired upload command"
        );

        let matches = cli_command(true)
            .try_get_matches_from(["elastic-diag", "host", "list"])
            .expect("hidden command still reaches profile validator");
        let host = Cli::from_arg_matches(&matches).expect("parse hidden host");
        assert_eq!(
            validate_extension_profile(host, true)
                .expect_err("host must be rejected")
                .kind(),
            ErrorKind::InvalidSubcommand
        );
    }

    #[test]
    fn extension_profile_parses_output_and_version_commands() {
        let output = parse_extension(&["elastic-diag", "output", "set", "monitoring"]).expect("output");
        assert!(matches!(output.command, Some(Commands::Output { .. })));

        let version = parse_extension(&["elastic-diag", "version"]).expect("version");
        assert!(matches!(version.command, Some(Commands::Version)));
    }

    #[test]
    fn bare_extension_process_gets_application_guidance() {
        let args = [OsString::from("elastic-diag"), OsString::from("process")];
        assert!(missing_extension_process_input(&args));

        let delegated_help = [
            OsString::from("elastic-diag"),
            OsString::from("help"),
            OsString::from("process"),
        ];
        assert!(!missing_extension_process_input(&delegated_help));
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
        let mut env = TestEnv::new();
        let state = TempDir::new().expect("temporary local state");
        std::fs::write(
            state.path().join(".env"),
            "STACK_MODE=core\nESDIAG_ELASTICSEARCH_PORT=19200\nESDIAG_KIBANA_PORT=15601\nESDIAG_PORT=12501\nESDIAG_OUTPUT_APIKEY=local-key\n",
        )
        .expect("write local state");
        env.set("ESDIAG_LOCAL_DIR", state.path());

        let preset = detected_esdiag_local_preset().expect("read core local state");
        assert_eq!(preset.elasticsearch_url, "http://localhost:19200");
        assert_eq!(preset.kibana_url, "http://localhost:15601");
        assert_eq!(preset.kibana_api_url, "http://localhost:15601");
        assert_eq!(preset.apikey.as_deref(), Some("local-key"));

        std::fs::write(state.path().join(".env"), "STACK_MODE=unknown\n").expect("write unsupported state");
        assert!(detected_esdiag_local_preset().is_none());
    }

    #[test]
    fn managed_full_container_preset_separates_internal_and_public_kibana_urls() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_CONTAINER_LOCAL_STACK", "full");
        env.set("ESDIAG_OUTPUT_URL", "http://elasticsearch:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "container-key");
        env.set("ESDIAG_KIBANA_INTERNAL_URL", "http://kibana:5601/s/esdiag");
        env.set("ESDIAG_KIBANA_PUBLIC_URL", "http://127.0.0.1:5601/s/esdiag");

        let preset = detected_esdiag_local_preset().expect("read full container state");
        assert_eq!(preset.elasticsearch_url, "http://elasticsearch:9200");
        assert_eq!(preset.kibana_api_url, "http://kibana:5601/s/esdiag");
        assert_eq!(preset.kibana_url, "http://127.0.0.1:5601/s/esdiag");
        assert_eq!(preset.apikey.as_deref(), Some("container-key"));
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
    fn collect_command_parses_send_id() {
        let cli = Cli::parse_from(["esdiag", "collect", "prod-es", "diag-dir", "--send", "abc123"]);
        match cli.command.expect("command") {
            Commands::Collect {
                host,
                output,
                send_id,
                user,
                ..
            } => {
                assert_eq!(host, "prod-es");
                assert_eq!(output, "diag-dir");
                assert_eq!(send_id, Some("abc123".to_string()));
                assert_eq!(user, None);
            }
            _ => panic!("expected collect command"),
        }
    }

    #[test]
    fn collect_command_keeps_user_short_option() {
        let cli = Cli::parse_from([
            "esdiag", "collect", "prod-es", "diag-dir", "-u", "elastic", "--send", "abc123",
        ]);
        match cli.command.expect("command") {
            Commands::Collect { send_id, user, .. } => {
                assert_eq!(user, Some("elastic".to_string()));
                assert_eq!(send_id, Some("abc123".to_string()));
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

    #[test]
    fn process_cli_parses_scrubbed_bool_override() {
        let cli = Cli::parse_from([
            "esdiag",
            "process",
            "local-diagnostics-scrubbed.zip",
            "--scrubbed",
            "true",
        ]);
        match cli.command.expect("command") {
            Commands::Process { scrubbed, .. } => {
                assert_eq!(scrubbed, Some(true));
            }
            _ => panic!("expected process command"),
        }
    }

    #[test]
    fn send_command_parses_file_and_upload_id() {
        let cli = Cli::parse_from(["esdiag", "send", "diag.zip", "abc123"]);
        match cli.command.expect("command") {
            Commands::Send {
                file_name,
                upload_id,
                api_url,
            } => {
                assert_eq!(file_name, "diag.zip");
                assert_eq!(upload_id, "abc123");
                assert_eq!(api_url, esdiag::elastic_upload_service::DEFAULT_UPLOAD_API_URL);
            }
            _ => panic!("expected send command"),
        }
        assert!(Cli::try_parse_from(["esdiag", "upload", "diag.zip", "abc123"]).is_err());
    }

    #[test]
    fn agent_mode_auto_enables_from_claudecode_env() {
        let mut env = TestEnv::new();
        env.set("CLAUDECODE", "1");

        let cli = Cli::parse_from(["esdiag", "keystore", "status"]);
        assert!(is_agent_mode(&cli));
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
    fn cli_commands_log_to_the_run_log_and_keep_the_terminal_quiet() {
        let cli = Cli {
            debug: false,
            agent: false,
            format: OutputFormat::Yaml,
            command: None,
        };
        let debug = Cli {
            debug: true,
            agent: false,
            format: OutputFormat::Yaml,
            command: None,
        };

        if std::env::var_os("LOG_LEVEL").is_none() {
            assert_eq!(resolve_tracing_filter(&cli).to_string(), "warn");
            assert_eq!(
                resolve_log_file_filter(&cli)
                    .map(|filter| filter.to_string())
                    .as_deref(),
                Some("info")
            );
        }
        assert_eq!(
            resolve_log_file_filter(&debug)
                .map(|filter| filter.to_string())
                .as_deref(),
            Some("debug")
        );
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_logs_to_stderr_without_a_run_log() {
        let serve = Cli::parse_from(["esdiag", "serve"]);

        assert!(resolve_log_file_filter(&serve).is_none());
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

    #[cfg(feature = "server")]
    #[test]
    fn serve_bind_resolves_flag_env_and_loopback_default() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_BIND");
        assert_eq!(resolve_serve_bind(None).expect("resolve bind"), Ipv4Addr::LOCALHOST);

        env.set("ESDIAG_BIND", "0.0.0.0");
        assert_eq!(resolve_serve_bind(None).expect("resolve bind"), Ipv4Addr::UNSPECIFIED);
        assert_eq!(
            resolve_serve_bind(Some(Ipv4Addr::LOCALHOST)).expect("resolve bind"),
            Ipv4Addr::LOCALHOST
        );

        env.set("ESDIAG_BIND", "localhost");
        let err = resolve_serve_bind(None).expect_err("invalid bind");
        assert!(err.to_string().contains("ESDIAG_BIND"));
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_prefers_explicit_flag_over_env() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_MODE", "service");

        let resolved = resolve_serve_runtime_mode(Some(RuntimeMode::User)).expect("resolve mode");

        assert_eq!(resolved, RuntimeMode::User);
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_uses_env_when_flag_missing() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_MODE", "service");

        let resolved = resolve_serve_runtime_mode(None).expect("resolve mode");

        assert_eq!(resolved, RuntimeMode::Service);
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_runtime_mode_defaults_to_user_without_flag_or_env() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_MODE");

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
    fn serve_parses_identity_header_flag() {
        let cli = Cli::parse_from(["esdiag", "serve", "--identity-header", "X-Authenticated-User"]);

        match cli.command {
            Some(Commands::Serve { identity_header, .. }) => {
                assert_eq!(identity_header.as_deref(), Some("X-Authenticated-User"));
            }
            other => panic!("expected serve command, got {other:?}"),
        }
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_requires_configuration_when_output_is_omitted() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_OUTPUT_URL");
        env.remove("ESDIAG_OUTPUT_APIKEY");
        env.remove("ESDIAG_OUTPUT_USERNAME");
        env.remove("ESDIAG_OUTPUT_PASSWORD");

        let err = match resolve_serve_exporter(None) {
            Ok(_) => panic!("omitted output without a configured deployment must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("No output deployment is configured"));
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_explicit_output_precedes_runtime_environment() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_OUTPUT_URL");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-secret");
        env.remove("ESDIAG_OUTPUT_USERNAME");
        env.remove("ESDIAG_OUTPUT_PASSWORD");

        let exporter = resolve_serve_exporter(Some("-".to_string())).expect("resolve explicit output");

        assert_eq!(exporter.target_uri(), "stdio://stdout");
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_uses_runtime_environment_when_output_is_omitted() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "http://localhost:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-secret");
        env.remove("ESDIAG_OUTPUT_USERNAME");
        env.remove("ESDIAG_OUTPUT_PASSWORD");

        let exporter = resolve_serve_exporter(None).expect("resolve runtime output");

        assert_eq!(exporter.target_uri(), "http://localhost:9200/");
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_rejects_partial_runtime_environment_without_leaking_secrets() {
        let mut env = TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "http://localhost:9200");
        env.remove("ESDIAG_OUTPUT_APIKEY");
        env.set("ESDIAG_OUTPUT_USERNAME", "do-not-print-this-secret");
        env.remove("ESDIAG_OUTPUT_PASSWORD");

        let err = match resolve_serve_exporter(None) {
            Ok(_) => panic!("partial output must fail closed"),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(message.contains("ESDIAG_OUTPUT_USERNAME and ESDIAG_OUTPUT_PASSWORD"));
        assert!(!message.contains("do-not-print-this-secret"));
    }
}
