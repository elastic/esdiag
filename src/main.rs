#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{Args, Parser, Subcommand, builder::BoolishValueParser, builder::styling};
#[cfg(feature = "server")]
use esdiag::server::{RuntimeMode, Server};
#[cfg(feature = "setup")]
use esdiag::setup;
use esdiag::{
    client::Client,
    data::{
        HostRole, KnownHost, KnownHostBuilder, KnownHostCliUpdate, Product, SecretAuth, Settings, Uri, add_secret,
        clear_unlock_lease, create_keystore, default_unlock_ttl, get_keystore_path, get_password_for_secret_commands,
        get_unlock_status, keystore_exists, parse_unlock_ttl, remove_secret, resolve_secret_auth,
        rotate_keystore_password, update_secret, validate_existing_keystore_password, write_unlock_lease,
    },
    env::LOG_LEVEL,
    exporter::Exporter,
    processor::{CollectionResult, Collector, DiagnosticReport, Identifiers, Processor, default_collect_archive_name},
    receiver::Receiver,
    uploader,
};
use eyre::{Result, eyre};
use std::{
    future::Future,
    io::{IsTerminal, Write},
    path::PathBuf,
    sync::Arc,
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

// Define command line arguments
#[derive(Debug, Parser)]
#[command(name = "esdiag", version, styles = STYLES)]
#[command(about = "Elastic Stack Diagnostics (esdiag) - collect diagnostics and import into Elasticsearch", long_about = None)]
struct Cli {
    /// Enable debug logging
    #[arg(global = true, long)]
    debug: bool,
    /// Enable agent-oriented low-noise CLI behavior
    #[arg(global = true, long, short = 'a')]
    agent: bool,
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
    /// Manage saved diagnostic jobs
    #[cfg(feature = "keystore")]
    Job {
        #[command(subcommand)]
        command: JobCommands,
    },
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
        /// Application of this host (elasticsearch, kibana, logstash, etc.)
        #[arg(help = "Application of this host (elasticsearch, kibana, logstash, etc.)")]
        app: Product,
        /// A host URL to connect to
        #[arg(help = "A host URL to connect to")]
        url: Url,
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
        /// Name of the saved host to test
        name: String,
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
            apikey: value.apikey,
            password: value.password,
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

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TOKIO_THREAD_STACK_SIZE)
        .build()?
        .block_on(async_main())
}

async fn async_main() -> Result<()> {
    // Parse CLI early to configure execution mode and logging.
    let cli = Cli::parse();
    let filter = resolve_tracing_filter(&cli);
    init_tracing(filter);

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        tracing::debug!("{:?}", panic);
        tracing::error!("{}", panic);
    }));

    clear_last_run_files()?;

    match run(cli).await {
        Ok(result) => {
            if let Some(summary) = result.summary() {
                emit_completion_summary(&summary)?;
            }
            if result.emit_summary && result.summary.is_none() {
                tracing::debug!("{} complete", result.name);
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!("{}", e);
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
    name: &'static str,
    summary: Option<String>,
    emit_summary: bool,
}

impl CommandResult {
    fn named(name: &'static str) -> Self {
        Self {
            name,
            summary: None,
            emit_summary: true,
        }
    }

    fn without_summary(name: &'static str) -> Self {
        Self {
            name,
            summary: None,
            emit_summary: false,
        }
    }

    fn with_summary(name: &'static str, summary: String) -> Self {
        Self {
            name,
            summary: Some(summary),
            emit_summary: true,
        }
    }

    fn summary(&self) -> Option<String> {
        if !self.emit_summary {
            return None;
        }
        Some(
            self.summary
                .clone()
                .unwrap_or_else(|| format!("{} complete", self.name)),
        )
    }
}

fn is_agent_mode(cli: &Cli) -> bool {
    cli.agent || std::env::var_os("CLAUDECODE").is_some()
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

fn write_completion_summary<W: Write>(writer: &mut W, summary: &str) -> std::io::Result<()> {
    writeln!(writer, "{summary}")
}

fn emit_completion_summary(summary: &str) -> Result<()> {
    let mut stderr = std::io::stderr();
    write_completion_summary(&mut stderr, summary)?;
    stderr.flush()?;
    Ok(())
}

fn format_process_summary(report: &DiagnosticReport, runtime_ms: u128) -> String {
    let mut summary = format!(
        "process complete in {:.3} seconds: {} documents for {}",
        runtime_ms as f64 / 1000.0,
        report.diagnostic.docs.created,
        report.diagnostic.metadata.id
    );
    if let Some(kibana_link) = report.diagnostic.kibana_link.as_deref() {
        summary.push_str(&format!("\nKibana Link: {kibana_link}"));
    }
    summary
}

fn format_collect_summary(result: &CollectionResult) -> String {
    format!(
        "Collected {} of {} files into {}",
        result.success, result.total, result.path
    )
}

#[tracing::instrument(skip_all)]
async fn run(cli: Cli) -> Result<CommandResult> {
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
                port,
                output,
                mode,
                web_features,
                kibana,
            } => {
                tracing::info!("Starting ESDiag server");
                let runtime_mode = resolve_serve_runtime_mode(mode)?;
                let exporter = resolve_serve_exporter(output, runtime_mode)?;

                let kibana_url = kibana.unwrap_or_else(|| {
                    esdiag::env::get_string("ESDIAG_KIBANA_URL")
                        .map(|url| esdiag::env::append_kibana_space(&url))
                        .unwrap_or_else(|_| "http://localhost:5601".to_string())
                });

                let (mut server, _bound_addr) = Server::start_with_web_features(
                    [0, 0, 0, 0],
                    port,
                    exporter,
                    kibana_url,
                    runtime_mode,
                    web_features.as_deref(),
                )
                .await?;

                wait_for_shutdown_signal().await?;

                server.shutdown().await;
                Ok(CommandResult::named("serve"))
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
                        let product = host.app().clone();
                        if let Some(sources) = sources {
                            esdiag::processor::init_sources(sources_product_key(&product)?, sources)?;
                        }
                        let known_host = Uri::try_from(host)?;
                        tracing::info!("Collecting diagnostic from {known_host}");
                        tracing::info!("Saving diagnostic to {output}");
                        let receiver = Receiver::try_from(known_host)?;
                        let output_dir = match output {
                            Uri::Directory(path) | Uri::File(path) => path,
                            _ => {
                                return Err(eyre!("Collect output must be a local directory path"));
                            }
                        };
                        let exporter = Exporter::for_collect_archive(output_dir)?;

                        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
                        let filename = format!("{}.zip", default_collect_archive_name(&product, &timestamp));
                        let identifiers = Identifiers::new(account, case, Some(filename), opportunity, user);

                        let collector =
                            Collector::try_new(receiver, exporter, product, r#type, include, exclude, identifiers)
                                .await?;
                        collect_with_optional_upload(
                            collector.collect(),
                            upload_id.as_deref(),
                            upload_collected_archive,
                        )
                        .await
                        .map(|result| CommandResult::with_summary("collect", format_collect_summary(&result)))
                    }
                    Uri::ElasticCloud(_) => Err(eyre!("Elastic Cloud API collection not yet implemented")),
                    _ => Err(eyre!("Collect requires a known host")),
                }
            }
            Commands::Host { command } => match command {
                HostCommands::Add { name, app, url, args } => {
                    tracing::info!("Adding host {name}");
                    if KnownHost::get_known(&name).is_some() {
                        return Err(eyre!("Host '{name}' already exists"));
                    }
                    let update = build_host_cli_update(args);
                    let secret_auth = resolve_host_secret_auth(update.secret.as_deref())?;
                    let host = build_host_from_definition(app, url, &update, secret_auth)?;
                    let summary = save_validated_host(&name, host, "added").await?;
                    Ok(CommandResult::with_summary("host add", summary))
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
                    let summary = save_validated_host(&name, host, "updated").await?;
                    Ok(CommandResult::with_summary("host update", summary))
                }
                HostCommands::Remove { name } => {
                    tracing::info!("Removing host {name}");
                    let hostfile = delete_host_from_cli(&name)?;
                    tracing::info!("Host {name} successfully deleted from {hostfile}");
                    Ok(CommandResult::with_summary(
                        "host remove",
                        format!("Host '{name}' removed from {hostfile}"),
                    ))
                }
                HostCommands::List => {
                    render_host_list()?;
                    Ok(CommandResult::without_summary("host list"))
                }
                HostCommands::Auth { name } => {
                    tracing::info!("Testing saved host {name}");
                    let host = KnownHost::get_known(&name).ok_or_else(|| eyre!("Host '{name}' not found"))?;
                    let uri = Uri::try_from(host)?;
                    let summary = validate_host_connection(&name, uri).await?;
                    Ok(CommandResult::with_summary("host auth", summary))
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
                    Ok(CommandResult::with_summary(
                        "keystore",
                        format_keystore_secret_summary("saved", &secret_id, &path),
                    ))
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
                    Ok(CommandResult::with_summary(
                        "keystore",
                        format_keystore_secret_summary("updated", &secret_id, &path),
                    ))
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
                    Ok(CommandResult::with_summary(
                        "keystore",
                        format_keystore_secret_summary("deleted", &secret_id, &path),
                    ))
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
                    let lock_status = format_keystore_lock_status(&status);
                    let rendered_lock_status =
                        colorize_keystore_lock_status(&lock_status, std::io::stderr().is_terminal());
                    Ok(CommandResult::with_summary("keystore", rendered_lock_status))
                }
                KeystoreCommands::Lock => {
                    let lock_status = format_keystore_lock_status(&esdiag::data::UnlockStatus {
                        keystore_exists: keystore_exists()?,
                        unlock_active: false,
                        expires_at_epoch: None,
                        unlock_path: esdiag::data::get_unlock_path()?,
                    });
                    let rendered_lock_status =
                        colorize_keystore_lock_status(&lock_status, std::io::stderr().is_terminal());
                    if clear_unlock_lease()? {
                        tracing::info!("Keystore unlock lease removed");
                    } else {
                        tracing::info!("Keystore unlock lease was already absent");
                    }
                    Ok(CommandResult::with_summary("keystore", rendered_lock_status))
                }
                KeystoreCommands::Status => {
                    let status = get_unlock_status()?;
                    let keystore_path = get_keystore_path()?;
                    let lock_status = format_keystore_lock_status(&status);
                    let rendered_lock_status =
                        colorize_keystore_lock_status(&lock_status, std::io::stderr().is_terminal());
                    tracing::info!(
                        "Keystore: {} ({})",
                        if status.keystore_exists { "present" } else { "absent" },
                        keystore_path.display()
                    );
                    tracing::info!("{rendered_lock_status}");
                    Ok(CommandResult::with_summary("keystore", rendered_lock_status))
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
                    Ok(CommandResult::with_summary(
                        "keystore",
                        format_keystore_password_summary(&path),
                    ))
                }
                KeystoreCommands::Migrate => {
                    let keystore_password = get_password_for_secret_commands()?;
                    let (migrated, unchanged) = KnownHost::migrate_hosts_to_keystore(&keystore_password)?;
                    tracing::info!(
                        "Keystore migration complete: migrated {migrated} host(s), unchanged {unchanged} host(s)."
                    );
                    Ok(CommandResult::with_summary(
                        "keystore",
                        format_keystore_migrate_summary(migrated, unchanged),
                    ))
                }
            },
            Commands::Process {
                input,
                output,
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
                let output_uri = Uri::try_from(output)?;
                ensure_uri_role(&input_uri, HostRole::Collect, "process input")?;
                if has_explicit_output {
                    ensure_uri_role(&output_uri, HostRole::Send, "process output")?;
                }

                tracing::info!("input: {}", input_uri);

                let receiver = Receiver::try_from(input_uri.clone())?;
                if let Some(sources) = sources {
                    let product = detect_sources_product_for_process(&input_uri, &receiver).await?;
                    esdiag::processor::init_sources(sources_product_key(&product)?, sources)?;
                }
                let receiver = Arc::new(receiver);
                let exporter = Arc::new(Exporter::try_from(output_uri)?);

                let identifiers = Identifiers::new(account, case, receiver.filename(), opportunity, user);
                let processor = Processor::try_new(receiver, exporter, identifiers).await?;
                let processor = match processor.start().await {
                    Ok(processor) => processor,
                    Err(processor) => {
                        return Err(eyre!("{}", processor));
                    }
                };

                match processor.process().await {
                    Ok(processor) => {
                        let summary = format_process_summary(&processor.state.report, processor.state.runtime);
                        Ok(CommandResult::with_summary("process", summary))
                    }
                    Err(processor) => {
                        tracing::info!(
                            "Process failed in {:.3} seconds",
                            processor.state.runtime as f64 / 1000.0
                        );
                        Err(eyre!("{}", processor))
                    }
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
                Ok(CommandResult::named("upload"))
            }
            #[cfg(feature = "setup")]
            Commands::Setup { host } => {
                if let Some(host) = host {
                    let uri = Uri::try_from(host)?;
                    let client = Client::try_from(uri)?;
                    tracing::info!("Setting up assets in {client}");
                    setup::assets(&client).await?;
                    Ok(CommandResult::named("setup"))
                } else {
                    tracing::debug!("Setting up assets with environment variables");
                    let es_uri = Uri::try_from_output_env()?;
                    let es_client = Client::try_from(es_uri)?;
                    tracing::info!("Setting up assets in {es_client}");
                    setup::assets(&es_client).await?;
                    let kb_uri = Uri::try_from_kibana_env()?;
                    let kb_client = Client::try_from(kb_uri)?;
                    tracing::info!("Setting up Kibana assets in {kb_client}");
                    setup::assets(&kb_client).await?;
                    Ok(CommandResult::named("setup"))
                }
            }
            #[cfg(feature = "keystore")]
            Commands::Job { command } => match command {
                JobCommands::List => {
                    esdiag::job::handle_job_list()?;
                    Ok(CommandResult::named("job list"))
                }
                JobCommands::Run { name } => {
                    esdiag::job::handle_job_run(&name).await?;
                    Ok(CommandResult::named("job run"))
                }
                JobCommands::Delete { name } => {
                    esdiag::job::handle_job_delete(&name)?;
                    Ok(CommandResult::named("job delete"))
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

            Ok(CommandResult {
                name: "tauri",
                summary: None,
                emit_summary: false,
            })
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

async fn collect_with_optional_upload<CollectFut, UploadFn, UploadFut>(
    collect_future: CollectFut,
    upload_id: Option<&str>,
    mut upload_fn: UploadFn,
) -> Result<CollectionResult>
where
    CollectFut: Future<Output = Result<CollectionResult>>,
    UploadFn: FnMut(PathBuf, String) -> UploadFut,
    UploadFut: Future<Output = Result<()>>,
{
    let result = collect_future.await?;
    if let Some(upload_id) = upload_id {
        upload_fn(PathBuf::from(&result.path), upload_id.to_string()).await?;
    }
    Ok(result)
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

async fn upload_collected_archive(file_path: PathBuf, upload_id: String) -> Result<()> {
    if !file_path.exists() {
        return Err(eyre!("Collected archive not found at {}", file_path.display()));
    }
    tracing::info!("Uploading collected archive {} to {}", file_path.display(), upload_id);
    let response = uploader::upload_file(&file_path, &upload_id, uploader::DEFAULT_UPLOAD_API_URL)
        .await
        .inspect_err(|_| {
            tracing::warn!(
                "Upload failed; collected archive remains available at {}",
                file_path.display()
            );
        })?;
    tracing::info!("Upload complete for slug {}", response.slug);
    Ok(())
}

fn sources_product_key(product: &Product) -> Result<&'static str> {
    esdiag::processor::diagnostic::data_source::source_product_key(product).map_err(|_| {
        eyre!(
            "--sources is only supported for Elasticsearch and Logstash inputs, got {}",
            product
        )
    })
}

async fn detect_sources_product_for_process(input_uri: &Uri, receiver: &Receiver) -> Result<Product> {
    match input_uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => Ok(host.app().clone()),
        _ => Ok(receiver.try_get_manifest_from_files().await?.product),
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
        (Some(apikey), None, None) => Ok(Some(SecretAuth::ApiKey { apikey })),
        (None, Some(username), Some(password)) => Ok(Some(SecretAuth::Basic { username, password })),
        _ => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
    }
}

fn build_host_cli_update(args: HostMutationArgs) -> KnownHostCliUpdate {
    args.into()
}

fn build_host_from_definition(
    app: Product,
    url: Url,
    update: &KnownHostCliUpdate,
    secret_auth: Option<SecretAuth>,
) -> Result<KnownHost> {
    let mut builder = KnownHostBuilder::new(url)
        .product(app)
        .accept_invalid_certs(update.accept_invalid_certs.unwrap_or(false))
        .apikey(update.apikey.clone())
        .username(update.username.clone())
        .password(update.password.clone())
        .secret(update.secret.clone());
    if let Some(roles) = update.roles.clone() {
        builder = builder.roles(roles);
    }
    match secret_auth {
        Some(secret_auth) => builder.build_with_secret_auth(secret_auth),
        None => builder.build(),
    }
}

async fn save_validated_host(name: &str, host: KnownHost, action: &str) -> Result<String> {
    let uri = Uri::try_from(host.clone())?;
    let validation_summary = validate_host_connection(name, uri).await?;
    let hostfile = host.save(name)?;
    tracing::info!("Host {name} successfully saved to {hostfile}");
    Ok(format!("{validation_summary}\nHost '{name}' {action} in {hostfile}"))
}

fn render_host_list() -> Result<()> {
    let rows = KnownHost::list_saved_summaries()?;
    if rows.is_empty() {
        println!("No saved hosts");
        return Ok(());
    }

    #[allow(clippy::literal_string_with_formatting_args)]
    let header = format!("{:<24} {:<16} {}", "name", "app", "secret");
    println!("{header}");
    println!("{}", "-".repeat(56));

    for row in rows {
        println!("{:<24} {:<16} {}", row.name, row.app, row.secret.unwrap_or_default());
    }

    Ok(())
}

fn legacy_host_command_error(args: &[String]) -> eyre::Report {
    let attempted = if args.is_empty() {
        "esdiag host".to_string()
    } else {
        format!("esdiag host {}", args.join(" "))
    };
    eyre!(
        "Legacy positional host syntax is no longer supported. Use `esdiag host add <name> <app> <url>` to create a host, `esdiag host update <name>` to modify one, `esdiag host remove <name>` to delete one, `esdiag host list` to inspect saved hosts, or `esdiag host auth <name>` to test a saved host. Received: `{attempted}`"
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
    value.and_then(|value| if value.trim().is_empty() { None } else { Some(value) })
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

fn format_keystore_lock_status(status: &esdiag::data::UnlockStatus) -> String {
    format_keystore_lock_status_at(chrono::Utc::now().timestamp(), status)
}

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

fn format_keystore_secret_summary(action: &str, secret_id: &str, path: &str) -> String {
    format!("Secret '{secret_id}' {action} in {path}")
}

fn format_keystore_password_summary(path: &str) -> String {
    format!("Keystore password updated for {path}")
}

fn format_keystore_migrate_summary(migrated: usize, unchanged: usize) -> String {
    format!("Keystore migration complete: migrated {migrated} host(s), unchanged {unchanged} host(s).")
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
fn resolve_serve_exporter(output: Option<String>, runtime_mode: RuntimeMode) -> Result<Exporter> {
    match output {
        Some(output) => Exporter::try_from(Uri::try_from(output)?),
        None if runtime_mode == RuntimeMode::User => Ok(Exporter::default()),
        None => Exporter::try_from(Uri::try_from_output_env()?),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, CommandResult, Commands, HostCommands, KeystoreCommands, collect_with_optional_upload,
        colorize_keystore_lock_status, format_collect_summary, format_keystore_lock_status,
        format_keystore_lock_status_at, format_keystore_migrate_summary, format_keystore_password_summary,
        format_keystore_secret_summary, format_process_summary, format_remaining_duration_from,
        host_connection_uses_receiver, is_agent_mode, resolve_host_secret_auth, resolve_secret_input_with_prompt,
        resolve_tracing_filter, should_error_for_missing_subcommand, upload_collected_archive,
        write_completion_summary,
    };
    #[cfg(feature = "keystore")]
    use super::{derive_collect_job, derive_process_job};
    #[cfg(feature = "server")]
    use super::{resolve_serve_exporter, resolve_serve_runtime_mode};
    use clap::Parser;
    use esdiag::data::{HostRole, JobAction, KnownHost, Product, SecretAuth, UnlockStatus, Uri, upsert_secret_auth};
    use esdiag::processor::diagnostic::DiagnosticReportBuilder;
    use esdiag::processor::{CollectionResult, DiagnosticManifest, Identifiers};
    #[cfg(feature = "server")]
    use esdiag::server::RuntimeMode;
    use std::sync::{
        Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    };
    use tempfile::TempDir;
    use url::Url;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
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

    #[test]
    fn host_add_parses_comma_delimited_role_values() {
        let cli = Cli::parse_from([
            "esdiag",
            "host",
            "add",
            "prod-es",
            "elasticsearch",
            "http://localhost:9200",
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
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        );
        host.save("prod-es").expect("save known host");

        let job = derive_collect_job("prod-es", "/tmp/esdiag-output", "support", None, Identifiers::default())
            .expect("derive collect job");

        assert_eq!(job.collect.save_dir, None);
        match job.action {
            JobAction::Collect { output_dir } => {
                assert_eq!(output_dir, std::path::PathBuf::from("/tmp/esdiag-output"));
            }
            _ => panic!("expected collect action"),
        }
    }

    #[cfg(feature = "keystore")]
    #[test]
    fn derive_process_job_requires_explicit_output() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        let host = KnownHost::new_no_auth(
            Product::Elasticsearch,
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

    #[tokio::test]
    async fn collect_with_optional_upload_uses_resolved_runtime_archive_path() {
        let upload_calls = AtomicUsize::new(0);
        let result = collect_with_optional_upload(
            std::future::ready(Ok(CollectionResult {
                path: "/tmp/runtime-generated-esdiag.zip".to_string(),
                success: 1,
                total: 1,
            })),
            Some("https://upload.elastic.co/g/abc123"),
            |path, upload_id| {
                upload_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(path, std::path::PathBuf::from("/tmp/runtime-generated-esdiag.zip"));
                assert_eq!(upload_id, "https://upload.elastic.co/g/abc123".to_string());
                std::future::ready(Ok(()))
            },
        )
        .await
        .expect("collect with upload succeeds");

        assert_eq!(result.path, "/tmp/runtime-generated-esdiag.zip");
        assert_eq!(upload_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn collect_with_optional_upload_skips_upload_when_collect_fails() {
        let upload_calls = AtomicUsize::new(0);
        let result = collect_with_optional_upload(
            std::future::ready(Err(eyre::eyre!("collect failed"))),
            Some("abc123"),
            |_path, _upload_id| {
                upload_calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok(()))
            },
        )
        .await;

        let err = match result {
            Ok(_) => panic!("collect failure should be returned"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("collect failed"));
        assert_eq!(upload_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn collect_with_optional_upload_skips_upload_when_no_upload_id_is_provided() {
        let upload_calls = AtomicUsize::new(0);
        let result = collect_with_optional_upload(
            std::future::ready(Ok(CollectionResult {
                path: "/tmp/collect-only-esdiag.zip".to_string(),
                success: 1,
                total: 1,
            })),
            None,
            |_path, _upload_id| {
                upload_calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok(()))
            },
        )
        .await
        .expect("collect without upload id succeeds");

        assert_eq!(result.path, "/tmp/collect-only-esdiag.zip");
        assert_eq!(upload_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn collect_with_optional_upload_returns_upload_error_and_keeps_archive() {
        let file = tempfile::Builder::new()
            .prefix("diag-")
            .suffix(".zip")
            .tempfile()
            .expect("temp file");
        let path = file.path().to_path_buf();
        let upload_calls = AtomicUsize::new(0);

        let result = collect_with_optional_upload(
            std::future::ready(Ok(CollectionResult {
                path: path.display().to_string(),
                success: 1,
                total: 1,
            })),
            Some("abc123"),
            |upload_path, upload_id| {
                upload_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(upload_path, path);
                assert_eq!(upload_id, "abc123".to_string());
                assert!(upload_path.exists(), "collected archive should still exist");
                std::future::ready(Err(eyre::eyre!("upload failed")))
            },
        )
        .await;

        let err = match result {
            Ok(_) => panic!("upload failure should be returned"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("upload failed"));
        assert_eq!(upload_calls.load(Ordering::SeqCst), 1);
        assert!(path.exists(), "upload failure should preserve collected archive");
    }

    #[tokio::test]
    async fn upload_collected_archive_returns_clear_error_when_file_is_missing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("missing-diag.zip");

        let err = upload_collected_archive(missing_path.clone(), "abc123".to_string())
            .await
            .expect_err("missing archive should fail");

        assert!(
            err.to_string()
                .contains(&format!("Collected archive not found at {}", missing_path.display()))
        );
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
            command: None,
        };

        assert_eq!(resolve_tracing_filter(&cli).to_string(), "debug");
    }

    #[test]
    fn agent_mode_uses_warn_filter_without_debug() {
        let cli = Cli {
            debug: false,
            agent: true,
            command: None,
        };

        assert_eq!(resolve_tracing_filter(&cli).to_string(), "warn");
    }

    #[test]
    fn write_completion_summary_writes_to_provided_writer() {
        let mut buffer = Vec::new();

        write_completion_summary(&mut buffer, "process complete").expect("write summary");

        assert_eq!(String::from_utf8(buffer).expect("utf8"), "process complete\n");
    }

    #[test]
    fn keystore_status_uses_lock_status_as_stderr_summary() {
        let result = CommandResult::with_summary("keystore", "Keystore: locked".to_string());

        assert_eq!(result.summary().as_deref(), Some("Keystore: locked"));
    }

    #[test]
    fn keystore_lock_uses_locked_stderr_summary() {
        let result = CommandResult::with_summary("keystore", colorize_keystore_lock_status("Keystore: locked", false));

        assert_eq!(result.summary().as_deref(), Some("Keystore: locked"));
    }

    #[test]
    fn keystore_unlock_uses_unlocked_stderr_summary() {
        let result = CommandResult::with_summary(
            "keystore",
            colorize_keystore_lock_status("Keystore: unlocked until later", false),
        );

        assert_eq!(result.summary().as_deref(), Some("Keystore: unlocked until later"));
    }

    #[test]
    fn keystore_secret_summary_is_useful() {
        assert_eq!(
            format_keystore_secret_summary("saved", "prod-es", "/tmp/secrets.yml"),
            "Secret 'prod-es' saved in /tmp/secrets.yml"
        );
    }

    #[test]
    fn keystore_password_summary_is_useful() {
        assert_eq!(
            format_keystore_password_summary("/tmp/secrets.yml"),
            "Keystore password updated for /tmp/secrets.yml"
        );
    }

    #[test]
    fn keystore_migrate_summary_is_useful() {
        assert_eq!(
            format_keystore_migrate_summary(2, 3),
            "Keystore migration complete: migrated 2 host(s), unchanged 3 host(s)."
        );
    }

    #[test]
    fn keystore_subcommands_do_not_fall_back_to_generic_summary() {
        let result = CommandResult::with_summary(
            "keystore",
            format_keystore_secret_summary("saved", "prod-es", "/tmp/secrets.yml"),
        );

        assert_ne!(result.summary().as_deref(), Some("keystore complete"));
    }

    #[test]
    fn collect_summary_uses_collected_counts_and_path() {
        let summary = format_collect_summary(&CollectionResult {
            path: "target/esdiag-20260403-220233.zip".to_string(),
            success: 21,
            total: 21,
        });

        assert_eq!(
            summary,
            "Collected 21 of 21 files into target/esdiag-20260403-220233.zip"
        );
    }

    #[test]
    fn process_summary_includes_kibana_link_for_stderr_output() {
        let manifest = DiagnosticManifest::new(
            "2024-01-01T00:00:00Z".to_string(),
            Some("esdiag-test".to_string()),
            None,
            None,
            Some("minimal".to_string()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some("8.15.0".to_string()),
        );
        let mut report = DiagnosticReportBuilder::try_from(manifest)
            .expect("report builder")
            .receiver("Elasticsearch http://localhost:9200".to_string())
            .product(Product::Elasticsearch)
            .build()
            .expect("report");
        report.add_kibana_link("https://kb.example/app/dashboards#/view/report".to_string());
        let summary = format_process_summary(&report, 1_234);

        let mut buffer = Vec::new();
        write_completion_summary(&mut buffer, &summary).expect("write summary");

        let stderr = String::from_utf8(buffer).expect("utf8");
        assert!(stderr.contains("process complete in 1.234 seconds"));
        assert!(stderr.contains("Kibana Link: https://kb.example/app/dashboards#/view/report"));
    }

    #[test]
    fn elastic_cloud_admin_hosts_validate_via_receiver() {
        let uri = Uri::ElasticCloudAdmin(KnownHost::new_legacy_apikey(
            Product::Elasticsearch,
            Url::parse("https://admin.found.no/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy")
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
            Product::Elasticsearch,
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
        upsert_secret_auth(
            "api-secret",
            SecretAuth::ApiKey {
                apikey: "secret-key".to_string(),
            },
            "pw",
        )
        .expect("save api secret");

        let resolved = resolve_host_secret_auth(Some("api-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_detects_basic() {
        let _guard = env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        upsert_secret_auth(
            "basic-secret",
            SecretAuth::Basic {
                username: "elastic".to_string(),
                password: "secret-password".to_string(),
            },
            "pw",
        )
        .expect("save basic secret");

        let resolved = resolve_host_secret_auth(Some("basic-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::Basic { .. })));
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
    fn serve_exporter_defaults_to_stdout_in_user_mode_without_output_env() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let exporter = resolve_serve_exporter(None, RuntimeMode::User).expect("resolve user-mode exporter");

        assert_eq!(exporter.target_uri(), "stdio://stdout");
    }

    #[cfg(feature = "server")]
    #[test]
    fn serve_exporter_requires_output_env_in_service_mode_without_output_flag() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_URL");
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let err = match resolve_serve_exporter(None, RuntimeMode::Service) {
            Ok(_) => panic!("service mode should still require configured output"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("ESDIAG_OUTPUT_URL is not defined"));
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
