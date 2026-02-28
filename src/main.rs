#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{Parser, Subcommand, builder::styling};
#[cfg(feature = "server")]
use esdiag::server::Server;
#[cfg(feature = "setup")]
use esdiag::setup;
use esdiag::{
    client::Client,
    data::{KnownHost, KnownHostBuilder, Product, Uri},
    env::LOG_LEVEL,
    exporter::{DirectoryExporter, Exporter},
    processor::{Collector, Identifiers, Processor},
    receiver::Receiver,
};
use eyre::{Result, eyre};
use std::sync::Arc;
#[cfg(feature = "server")]
use tokio::signal::unix::{SignalKind, signal};
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
    /// Override the path to sources.yml
    #[arg(global = true, long)]
    sources: Option<String>,
    /// Commands
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Collect a diagnostic bundle from a known host's API endpoints, writes output to a directory
    Collect {
        /// The host to collect diagnostics from
        #[arg(help = "The Elasticsearch host to collect diagnostics from")]
        host: String,
        /// The output directory to save the diagnostics to
        #[arg(help = "An existing directory to create a diagnostic directory and files in")]
        output: String,
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
        #[arg(help = "Application of this host (elasticsearch, kibana, logstash, etc.)")]
        app: Option<Product>,
        /// A host URL to connect to
        #[arg(help = "A host URL to connect to")]
        url: Option<Url>,
        /// Accept invalid certificates
        #[arg(help = "Accept invalid certificates", long)]
        accept_invalid_certs: bool,
        /// ApiKey for authentication
        #[arg(help = "ApiKey, passed as http header ", long, short, conflicts_with_all = &["username", "password"])]
        apikey: Option<String>,
        /// Username for authentication
        #[arg(help = "Username for authentication", long, short)]
        username: Option<String>,
        /// Password for authentication
        #[arg(help = "Password for authentication", long, short)]
        password: Option<String>,
        /// Save the host configuration
        #[arg(
            help = "Don't save the host configuration on succesful connection",
            long,
            short
        )]
        nosave: bool,
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

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // Parse CLI early to check for debug flag
    let cli = Cli::parse();

    // Initialize logging with debug override if flag is set
    if cli.debug {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .format_timestamp_millis()
            .init();
    } else {
        let env = env_logger::Env::default().filter_or("LOG_LEVEL", LOG_LEVEL);
        env_logger::Builder::from_env(env)
            .format_timestamp_millis()
            .init();
    }

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        log::debug!("{:?}", panic);
        log::error!("{}", panic);
    }));

    clear_last_run_files()?;

    if let Some(sources) = cli.sources.clone() {
        esdiag::processor::init_sources(Some(sources))?;
    }

    match run(cli).await {
        Ok(cmd) => {
            log::debug!("{cmd} complete");
            Ok(())
        }
        Err(e) => {
            log::error!("{}", e);
            Err(eyre!(e))
        }
    }
}

async fn run(cli: Cli) -> Result<&'static str> {
    match cli.command {
        #[cfg(feature = "server")]
        Commands::Serve {
            port,
            output,
            kibana,
        } => {
            log::info!("Starting ESDiag server");

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

            let mut server = Server::new(port, exporter, kibana_url);

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Shutting down server (Ctrl+C)...");
                }
                _ = async {
                    let mut term_signal = signal(SignalKind::terminate()).map_err(|e| eyre!("Failed to install SIGTERM handler: {}", e))?;
                    term_signal.recv().await;
                    log::info!("Shutting down server (SIGTERM)...");
                    Ok::<_, eyre::Report>(())
                } => {}
            }

            server.shutdown().await;
            Ok("serve")
        }
        Commands::Collect { host, output } => {
            let known_host = Uri::try_from(host)?;
            let output = Uri::try_from(output)?;
            match known_host {
                Uri::KnownHost(_) | Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_) => {
                    log::info!("Collecting diagnostic from {known_host}");
                    log::info!("Saving diagnostic to {output}");
                    let receiver = Receiver::try_from(known_host)?;
                    let exporter = DirectoryExporter::try_from(output)?;
                    let collector = Collector::try_new(receiver, exporter).await?;
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
            apikey,
            username,
            password,
            nosave,
        } => {
            log::info!("Configuring host {name}");
            let host = if let (Some(app), Some(url)) = (app, url) {
                KnownHostBuilder::new(url)
                    .product(app)
                    .accept_invalid_certs(accept_invalid_certs)
                    .apikey(apikey)
                    .username(username)
                    .password(password)
                    .build()?
            } else {
                KnownHost::get_known(&name).ok_or(eyre!(
                    "Host {name} not found, must include `url` and `app` to setup a new host."
                ))?
            };

            let uri = Uri::try_from(host.clone())?;

            let valid_connection = match Client::try_from(uri)?.test_connection().await {
                Ok(message) => {
                    log::info!("Host {name}: {}", &message);
                    true
                }
                Err(message) => {
                    log::error!("Host connection: FAILED ❌ {}", &message);
                    log::warn!("Check your URL and certificates!");
                    false
                }
            };

            if valid_connection {
                if !nosave {
                    let hostfile = host.save(&name)?;
                    log::info!("Host {name} successfully saved to {hostfile}");
                }
                Ok("host")
            } else {
                Err(eyre!("Host connection failed"))
            }
        }
        Commands::Process {
            input,
            output,
            account,
            case,
            opportunity,
            user,
        } => {
            let input_uri = Uri::try_from(input)?;
            let output_uri = Uri::try_from(output)?;

            log::info!("input: {}", input_uri);

            let receiver = Arc::new(Receiver::try_from(input_uri)?);
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
                    log::info!(
                        "Process complete in {:.3} seconds",
                        processor.state.runtime as f64 / 1000.0
                    );
                    Ok("process")
                }
                Err(processor) => {
                    log::info!(
                        "Process failed in {:.3} seconds",
                        processor.state.runtime as f64 / 1000.0
                    );
                    Err(eyre!("{}", processor))
                }
            }
        }
        #[cfg(feature = "setup")]
        Commands::Setup { host } => {
            if let Some(host) = host {
                let uri = Uri::try_from(host)?;
                let client = Client::try_from(uri)?;
                log::info!("Setting up assets in {client}");
                setup::assets(&client).await?;
                Ok("setup")
            } else {
                log::debug!("Setting up assets with environment variables");
                let es_uri = Uri::try_from_output_env()?;
                let es_client = Client::try_from(es_uri)?;
                log::info!("Setting up assets in {es_client}");
                setup::assets(&es_client).await?;
                let kb_uri = Uri::try_from_kibana_env()?;
                let kb_client = Client::try_from(kb_uri)?;
                log::info!("Setting up Kibana assets in {kb_client}");
                setup::assets(&kb_client).await?;
                Ok("setup")
            }
        }
    }
}

fn clear_last_run_files() -> Result<()> {
    let home_dir = match std::env::consts::OS {
        "windows" => std::env::var("USERPROFILE")?,
        "linux" | "macos" => std::env::var("HOME")?,
        os => return Err(eyre!("Unknown home directory for operating system: {os} ")),
    };
    log::debug!("Home directory is: {home_dir}");
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
        log::debug!("Removing {}", &file.display());
        // Ignore "file not found" errors on delete
        let _ = std::fs::remove_file(file);
    }
    Ok(())
}
