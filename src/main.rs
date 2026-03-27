#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{Parser, Subcommand, builder::BoolishValueParser, builder::styling};
#[cfg(feature = "server")]
use esdiag::server::{RuntimeMode, Server};
#[cfg(feature = "setup")]
use esdiag::setup;
use esdiag::{
    client::Client,
    data::{
        HostRole, KnownHost, KnownHostBuilder, Product, SecretAuth, Uri, add_secret,
        clear_unlock_lease, create_keystore, default_unlock_ttl, get_password_for_secret_commands,
        get_unlock_status, keystore_exists, parse_unlock_ttl, remove_secret,
        resolve_secret_auth, rotate_keystore_password, update_secret,
        validate_existing_keystore_password, write_unlock_lease,
        KnownHostCliUpdate, Settings,
    },
    env::LOG_LEVEL,
    exporter::Exporter,
    processor::{Collector, Identifiers, Processor},
    receiver::Receiver,
    uploader,
};
use eyre::{Result, eyre};
use std::{io::{IsTerminal, Write}, sync::Arc, time::Duration};
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
        #[arg(
            long,
            help = "Comma-separated list of APIs to include",
            value_delimiter = ','
        )]
        include: Option<Vec<String>>,
        /// Explicitly exclude APIs
        #[arg(
            long,
            help = "Comma-separated list of APIs to exclude",
            value_delimiter = ','
        )]
        exclude: Option<Vec<String>>,
        /// Override the embedded sources.yml for the detected Elasticsearch or Logstash workflow.
        /// The file must match the active product or the command fails before collection.
        #[arg(long)]
        sources: Option<String>,
        /// Diagnostic report account name
        #[arg(help = "Diagnostic report account name", long, short)]
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
    },
    /// Start a web server to receive diagnostic bundle uploads
    #[cfg(feature = "server")]
    Serve {
        /// The port to bind the server to
        #[arg(
            help = "The port to bind the server to",
            long,
            short,
            default_value = "2501"
        )]
        port: u16,
        /// Target to send processed diagnostic documents to
        #[arg(
            long_help = "Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD."
        )]
        output: Option<String>,
        /// Web runtime mode for the server
        #[arg(long, value_enum, help = "Web runtime mode: user or service")]
        mode: Option<RuntimeMode>,
        /// Kibana URL to display in the web interface
        #[arg(
            long,
            long_help = "Kibana URL to display in the web interface. If not provided, will use the ESDIAG_KIBANA_URL environment variable."
        )]
        kibana: Option<String>,
    },
    /// Configure, test and save a remote host connection to `~/.esdiag/hosts.yml`
    Host {
        /// A name to identify this host
        #[arg(help = "A name to identify this host")]
        name: String,
        /// Application of this host (elasticsearch, kibana, logstash, etc.)
        #[arg(help = "Application of this host (elasticsearch, kibana, logstash, etc.)", conflicts_with = "delete")]
        app: Option<Product>,
        /// A host URL to connect to
        #[arg(help = "A host URL to connect to", conflicts_with = "delete")]
        url: Option<Url>,
        /// Accept invalid certificates
        #[arg(help = "Accept invalid certificates", long, value_parser = BoolishValueParser::new(), conflicts_with = "delete")]
        accept_invalid_certs: Option<bool>,
        /// Delete the saved host configuration
        #[arg(help = "Delete the saved host configuration", long, conflicts_with_all = &["app", "url", "accept_invalid_certs", "apikey", "username", "password", "secret", "roles", "nosave"])]
        delete: bool,
        /// ApiKey for authentication
        #[arg(help = "ApiKey, passed as http header ", long, short, conflicts_with_all = &["username", "password", "delete"])]
        apikey: Option<String>,
        /// Username for authentication
        #[arg(
            help = "Username for authentication",
            long = "user",
            visible_alias = "username",
            short,
            conflicts_with = "delete"
        )]
        username: Option<String>,
        /// Password for authentication
        #[arg(help = "Password for authentication", long, short, conflicts_with = "delete")]
        password: Option<String>,
        /// Secret identifier in the encrypted keystore
        #[arg(
            help = "Secret identifier in the encrypted keystore",
            long,
            conflicts_with_all = &["apikey", "username", "password", "delete"]
        )]
        secret: Option<String>,
        /// Comma-separated host roles (collect,send,view)
        #[arg(help = "Comma-separated host roles", long, value_delimiter = ',', conflicts_with = "delete")]
        roles: Option<Vec<HostRole>>,
        /// Save the host configuration
        #[arg(
            help = "Don't save the host configuration on successful connection",
            long,
            short,
            conflicts_with = "delete"
        )]
        nosave: bool,
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
            help = "Source to read diagnostic data from (archive, directory, known host or Elastic uploader URL)"
        )]
        input: String,

        /// Target to send processed diagnostic documents to
        #[arg(
            long_help = "Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD."
        )]
        output: Option<String>,

        /// Diagnostic report account name
        #[arg(help = "Diagnostic report account name", long, short)]
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
        /// Override the embedded sources.yml for the detected Elasticsearch or Logstash workflow.
        /// The file must match the active product or the command fails before processing.
        #[arg(long)]
        sources: Option<String>,
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
            short,
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
            short,
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
            help = "ApiKey, passed as http header ",
            long,
            short,
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
    // Parse CLI early to check for debug flag
    let cli = Cli::parse();

    // Initialize tracing subscriber with debug override if flag is set
    let filter = if cli.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new(LOG_LEVEL))
    };
    init_tracing(filter);

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        tracing::debug!("{:?}", panic);
        tracing::error!("{}", panic);
    }));

    clear_last_run_files()?;

    match run(cli).await {
        Ok(cmd) => {
            tracing::debug!("{cmd} complete");
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

    let subscriber = fmt().with_env_filter(filter).finish();
    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("tracing subscriber already initialized: {err}");
    }
}

#[tracing::instrument(skip_all)]
async fn run(cli: Cli) -> Result<&'static str> {
    // If there are CLI arguments but no subcommand, avoid starting the desktop/Tauri
    // entrypoint. The desktop UI should only start when launched absolutely without arguments.
    if should_error_for_missing_subcommand(std::env::args_os().len(), cli.command.is_none()) {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        cmd.print_help()?;
        return Err(eyre!(
            "No subcommand provided. Use --help for usage information."
        ));
    }

    if let Some(command) = cli.command {
        match command {
            #[cfg(feature = "server")]
            Commands::Serve {
                port,
                output,
                mode,
                kibana,
            } => {
                tracing::info!("Starting ESDiag server");

                let output_uri = Uri::try_from(output)?;
                let exporter = Exporter::try_from(output_uri)?;

                let kibana_url = kibana.unwrap_or_else(|| {
                    let url = esdiag::env::get_string("ESDIAG_KIBANA_URL")
                        .unwrap_or_else(|_| "http://localhost:5601".to_string());
                    match esdiag::env::get_string("ESDIAG_KIBANA_SPACE").ok() {
                        Some(space) => format!("{url}/s/{space}"),
                        None => url,
                    }
                });

                let runtime_mode = resolve_serve_runtime_mode(mode)?;
                let (mut server, _bound_addr) =
                    Server::start([0, 0, 0, 0], port, exporter, kibana_url, runtime_mode).await?;

                wait_for_shutdown_signal().await?;

                server.shutdown().await;
                Ok("serve")
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
            } => {
                let known_host = Uri::try_from(host)?;
                let output = Uri::try_from(output)?;
                match known_host {
                    Uri::KnownHost(host)
                    | Uri::ElasticCloudAdmin(host)
                    | Uri::ElasticGovCloudAdmin(host) => {
                        ensure_host_role(&host, HostRole::Collect, "collect")?;
                        let product = host.app().clone();
                        if let Some(sources) = sources {
                            esdiag::processor::init_sources(
                                sources_product_key(&product)?,
                                sources,
                            )?;
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

                        let filename =
                            format!("esdiag-{}.zip", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                        let identifiers =
                            Identifiers::new(account, case, Some(filename), opportunity, user);

                        let collector = Collector::try_new(
                            receiver,
                            exporter,
                            product,
                            r#type,
                            include,
                            exclude,
                            identifiers,
                        )
                        .await?;
                        collector.collect().await?;
                        Ok("collect")
                    }
                    Uri::ElasticCloud(_) => {
                        Err(eyre!("Elastic Cloud API collection not yet implemented"))
                    }
                    _ => Err(eyre!("Collect requires a known host")),
                }
            }
            Commands::Host {
                name,
                app,
                url,
                accept_invalid_certs,
                delete,
                apikey,
                username,
                password,
                secret,
                roles,
                nosave,
            } => {
                tracing::info!("Configuring host {name}");
                let update = build_host_cli_update(
                    accept_invalid_certs,
                    apikey,
                    username,
                    password,
                    secret,
                    roles,
                );
                let mode = resolve_host_command_mode(delete, &app, &url, &update)?;

                if mode == HostCommandMode::Delete {
                    let hostfile = delete_host_from_cli(&name)?;
                    tracing::info!("Host {name} successfully deleted from {hostfile}");
                    return Ok("host");
                }

                let host = match mode {
                    HostCommandMode::CreateOrReplace => {
                        let secret_auth = resolve_host_secret_auth(update.secret.as_deref())?;
                        build_host_from_definition(
                            app.expect("app required for create-or-replace"),
                            url.expect("url required for create-or-replace"),
                            &update,
                            secret_auth,
                        )?
                    }
                    HostCommandMode::IncrementalUpdate => {
                        let existing = KnownHost::get_known(&name).ok_or_else(|| {
                            eyre!(
                                "Host {name} not found, must include `url` and `app` to setup a new host."
                            )
                        })?;
                        let secret_auth = if update.secret.is_some() {
                            resolve_host_secret_auth(update.secret.as_deref())?
                        } else {
                            None
                        };
                        existing.merge_cli_update(&update, secret_auth)?
                    }
                    HostCommandMode::ValidateOnly => KnownHost::get_known(&name).ok_or_else(|| {
                        eyre!(
                            "Host {name} not found, must include `url` and `app` to setup a new host."
                        )
                    })?,
                    HostCommandMode::Delete => unreachable!("delete handled earlier"),
                };

                let uri = Uri::try_from(host.clone())?;

                let valid_connection = validate_host_connection(&name, uri).await?;

                if valid_connection {
                    if !nosave {
                        let hostfile = host.save(&name)?;
                        tracing::info!("Host {name} successfully saved to {hostfile}");
                    }
                    Ok("host")
                } else {
                    Err(eyre!("Host connection failed"))
                }
            }
            Commands::Keystore { command } => {
                match command {
                    KeystoreCommands::Add {
                        secret_id,
                        username,
                        password,
                        apikey,
                    } => {
                        let keystore_password = get_password_for_secret_commands()?;
                        let (username, password, apikey) =
                            resolve_secret_input(username, password, apikey)?;
                        let path =
                            add_secret(&secret_id, username, password, apikey, &keystore_password)?;
                        tracing::info!("Secret '{secret_id}' saved to {path}");
                        Ok("keystore")
                    }
                    KeystoreCommands::Update {
                        secret_id,
                        username,
                        password,
                        apikey,
                    } => {
                        let keystore_password = get_password_for_secret_commands()?;
                        let (username, password, apikey) =
                            resolve_secret_input(username, password, apikey)?;
                        let path = update_secret(
                            &secret_id,
                            username,
                            password,
                            apikey,
                            &keystore_password,
                        )?;
                        tracing::info!("Secret '{secret_id}' updated in {path}");
                        Ok("keystore")
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
                        Ok("keystore")
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
                        Ok("keystore")
                    }
                    KeystoreCommands::Lock => {
                        if clear_unlock_lease()? {
                            tracing::info!("Keystore unlock lease removed");
                        } else {
                            tracing::info!("Keystore unlock lease was already absent");
                        }
                        Ok("keystore")
                    }
                    KeystoreCommands::Status => {
                        let status = get_unlock_status()?;
                        tracing::info!(
                            "Keystore: {} ({})",
                            if status.keystore_exists {
                                "present"
                            } else {
                                "absent"
                            },
                            status.unlock_path.display()
                        );
                        if status.unlock_active {
                            if let Some(expires_at_epoch) = status.expires_at_epoch {
                                tracing::info!(
                                    "CLI unlock: active until {} ({})",
                                    format_epoch(expires_at_epoch),
                                    format_remaining_duration(expires_at_epoch)
                                );
                            } else {
                                tracing::info!("CLI unlock: active");
                            }
                        } else {
                            tracing::info!("CLI unlock: inactive");
                        }
                        Ok("keystore")
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
                        Ok("keystore")
                    }
                    KeystoreCommands::Migrate => {
                        let keystore_password = get_password_for_secret_commands()?;
                        let (migrated, unchanged) =
                            KnownHost::migrate_hosts_to_keystore(&keystore_password)?;
                        tracing::info!(
                            "Keystore migration complete: migrated {migrated} host(s), unchanged {unchanged} host(s)."
                        );
                        Ok("keystore")
                    }
                }
            }
            Commands::Process {
                input,
                output,
                account,
                case,
                opportunity,
                user,
                sources,
            } => {
                let has_explicit_output = output.is_some();
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

                let identifiers =
                    Identifiers::new(account, case, receiver.filename(), opportunity, user);
                let processor = Processor::try_new(receiver, exporter, identifiers).await?;
                let processor = match processor.start().await {
                    Ok(processor) => processor,
                    Err(processor) => {
                        return Err(eyre!("{}", processor));
                    }
                };

                match processor.process().await {
                    Ok(processor) => {
                        tracing::info!(
                            "Process complete in {:.3} seconds",
                            processor.state.runtime as f64 / 1000.0
                        );
                        Ok("process")
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
                Ok("upload")
            }
            #[cfg(feature = "setup")]
            Commands::Setup { host } => {
                if let Some(host) = host {
                    let uri = Uri::try_from(host)?;
                    let client = Client::try_from(uri)?;
                    tracing::info!("Setting up assets in {client}");
                    setup::assets(&client).await?;
                    Ok("setup")
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
                    Ok("setup")
                }
            }
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
                            if let Ok(host) = esdiag::data::KnownHost::get_known(target)
                                .ok_or_else(|| eyre::eyre!("Host not found"))
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
                            match esdiag::env::get_string("ESDIAG_KIBANA_SPACE").ok() {
                                Some(space) => format!("{url}/s/{space}"),
                                None => url,
                            }
                        });

                        let (mut server, bound_addr) = match Server::start(
                            [127, 0, 0, 1],
                            0,
                            exporter,
                            kibana_url,
                            RuntimeMode::User,
                        )
                        .await
                        {
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

            Ok("tauri")
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

fn sources_product_key(product: &Product) -> Result<&'static str> {
    esdiag::processor::diagnostic::data_source::source_product_key(product).map_err(|_| {
        eyre!(
            "--sources is only supported for Elasticsearch and Logstash inputs, got {}",
            product
        )
    })
}

async fn detect_sources_product_for_process(
    input_uri: &Uri,
    receiver: &Receiver,
) -> Result<Product> {
    match input_uri {
        Uri::KnownHost(host) | Uri::ElasticCloudAdmin(host) | Uri::ElasticGovCloudAdmin(host) => {
            Ok(host.app().clone())
        }
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
        (None, Some(username), Some(password)) => {
            Ok(Some(SecretAuth::Basic { username, password }))
        }
        _ => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostCommandMode {
    CreateOrReplace,
    Delete,
    IncrementalUpdate,
    ValidateOnly,
}

fn build_host_cli_update(
    accept_invalid_certs: Option<bool>,
    apikey: Option<String>,
    username: Option<String>,
    password: Option<String>,
    secret: Option<String>,
    roles: Option<Vec<HostRole>>,
) -> KnownHostCliUpdate {
    KnownHostCliUpdate {
        accept_invalid_certs,
        apikey,
        password,
        roles,
        secret,
        username,
    }
}

fn resolve_host_command_mode(
    delete: bool,
    app: &Option<Product>,
    url: &Option<Url>,
    update: &KnownHostCliUpdate,
) -> Result<HostCommandMode> {
    if app.is_some() ^ url.is_some() {
        return Err(eyre!(
            "Host definition requires both `app` and `url` when creating or replacing a host."
        ));
    }

    if delete {
        return Ok(HostCommandMode::Delete);
    }

    if app.is_some() && url.is_some() {
        return Ok(HostCommandMode::CreateOrReplace);
    }

    if update.is_empty() {
        Ok(HostCommandMode::ValidateOnly)
    } else {
        Ok(HostCommandMode::IncrementalUpdate)
    }
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
    let requested_password_prompt = password
        .as_ref()
        .is_some_and(|value| value.trim().is_empty());
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
    value.and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        }
    })
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
        return Err(eyre!(
            "A new keystore password requires an interactive terminal."
        ));
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
    matches!(
        uri,
        Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_)
    )
}

async fn validate_host_connection(name: &str, uri: Uri) -> Result<bool> {
    if host_connection_uses_receiver(&uri) {
        let receiver = Receiver::try_from(uri)?;
        if receiver.is_connected().await {
            tracing::info!("Host {name}: connected to Elastic Cloud Admin proxy");
            return Ok(true);
        }

        tracing::error!("Host connection: FAILED ❌ Elastic Cloud Admin proxy connection failed");
        tracing::warn!("Check your URL, certificates, and secret credentials!");
        return Ok(false);
    }

    match Client::try_from(uri)?.test_connection().await {
        Ok(message) => {
            tracing::info!("Host {name}: {}", &message);
            Ok(true)
        }
        Err(message) => {
            tracing::error!("Host connection: FAILED ❌ {}", &message);
            tracing::warn!("Check your URL and certificates!");
            Ok(false)
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

#[cfg(test)]
mod tests {
    #[cfg(feature = "server")]
    use super::resolve_serve_runtime_mode;
    use super::{
        Cli, Commands, KeystoreCommands, format_remaining_duration_from,
        host_connection_uses_receiver, resolve_host_secret_auth,
        resolve_secret_input_with_prompt, should_error_for_missing_subcommand,
    };
    use clap::Parser;
    use esdiag::data::{
        ElasticCloud, HostRole, KnownHost, Product, SecretAuth, Uri, upsert_secret_auth,
    };
    #[cfg(feature = "server")]
    use esdiag::server::RuntimeMode;
    use std::sync::{Mutex, OnceLock};
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
    fn host_roles_cli_parses_comma_delimited_values() {
        let cli = Cli::parse_from([
            "esdiag",
            "host",
            "prod-es",
            "elasticsearch",
            "http://localhost:9200",
            "--roles",
            "collect,send",
        ]);
        match cli.command.expect("command") {
            Commands::Host { roles, .. } => {
                assert_eq!(roles, Some(vec![HostRole::Collect, HostRole::Send]));
            }
            _ => panic!("expected host command"),
        }
    }

    #[test]
    fn host_accept_invalid_certs_cli_parses_explicit_bool_values() {
        let cli_true = Cli::parse_from([
            "esdiag",
            "host",
            "prod-es",
            "--accept-invalid-certs",
            "true",
        ]);
        match cli_true.command.expect("command") {
            Commands::Host {
                accept_invalid_certs,
                ..
            } => {
                assert_eq!(accept_invalid_certs, Some(true));
            }
            _ => panic!("expected host command"),
        }

        let cli_false = Cli::parse_from([
            "esdiag",
            "host",
            "prod-es",
            "--accept-invalid-certs",
            "false",
        ]);
        match cli_false.command.expect("command") {
            Commands::Host {
                accept_invalid_certs,
                ..
            } => {
                assert_eq!(accept_invalid_certs, Some(false));
            }
            _ => panic!("expected host command"),
        }
    }

    #[test]
    fn host_delete_conflicts_with_update_fields() {
        let result = Cli::try_parse_from([
            "esdiag",
            "host",
            "prod-es",
            "--delete",
            "--secret",
            "rotated",
        ]);
        assert!(result.is_err(), "delete should conflict with update fields");
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
        let resolved = resolve_secret_input_with_prompt(
            None,
            None,
            Some(String::new()),
            |prompt| {
                prompts.push(prompt.to_string());
                Ok("prompted-api-key".to_string())
            },
        )
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
    fn elastic_cloud_admin_hosts_validate_via_receiver() {
        let uri = Uri::ElasticCloudAdmin(KnownHost::ApiKey {
            accept_invalid_certs: false,
            apikey: None,
            app: Product::Elasticsearch,
            cloud_id: Some(ElasticCloud::ElasticCloudAdmin),
            roles: vec![HostRole::Collect],
            secret: Some("ada-admin".to_string()),
            viewer: None,
            url: Url::parse(
                "https://admin.found.no/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy",
            )
            .expect("valid url"),
        });

        assert!(host_connection_uses_receiver(&uri));
    }

    #[test]
    fn standard_known_hosts_validate_via_client() {
        let uri = Uri::KnownHost(KnownHost::NoAuth {
            accept_invalid_certs: false,
            app: Product::Elasticsearch,
            roles: vec![HostRole::Collect],
            viewer: None,
            url: Url::parse("http://localhost:9200").expect("valid url"),
        });

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
}

#[cfg(all(test, feature = "server", feature = "desktop"))]
mod desktop_startup_tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn embedded_server_starts_and_serves_local_url() {
        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) =
            Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
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
        let occupied_listener =
            TcpListener::bind("127.0.0.1:0").expect("should reserve a local test port");
        let occupied_port = occupied_listener
            .local_addr()
            .expect("reserved listener has local addr")
            .port();

        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) =
            Server::start([127, 0, 0, 1], 0, exporter, kibana_url, RuntimeMode::User)
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
