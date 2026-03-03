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
    exporter::Exporter,
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
    command: Option<Commands>,
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

                let (mut server, _bound_addr) = Server::start([0, 0, 0, 0], port, exporter, kibana_url).await?;

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
            Commands::Collect {
                host,
                output,
                r#type,
                include,
                exclude,
                account,
                case,
                opportunity,
                user,
            } => {
                let known_host = Uri::try_from(host)?;
                let output = Uri::try_from(output)?;
                match known_host {
                    Uri::KnownHost(_) | Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_) => {
                        log::info!("Collecting diagnostic from {known_host}");
                        log::info!("Saving diagnostic to {output}");
                        let receiver = Receiver::try_from(known_host)?;
                        let output_dir = match output {
                            Uri::Directory(path) | Uri::File(path) => path,
                            _ => return Err(eyre!("Collect output must be a local directory path")),
                        };
                        let exporter = Exporter::for_collect_archive(output_dir)?;

                        let filename =
                            format!("esdiag-{}.zip", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                        let identifiers =
                            Identifiers::new(account, case, Some(filename), opportunity, user);

                        let collector = Collector::try_new(
                            receiver,
                            exporter,
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
                            if let Ok(host) = esdiag::data::KnownHost::get_known(target).ok_or_else(|| eyre::eyre!("Host not found")) {
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
                        
                        let (mut server, bound_addr) = match Server::start([127, 0, 0, 1], 0, exporter, kibana_url).await {
                            Ok(res) => res,
                            Err(e) => {
                                log::error!("Failed to start embedded server: {}", e);
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
            Err(eyre!("No command provided. If you want to use the Desktop UI, compile with the 'desktop' feature."))
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

#[cfg(test)]
mod tests {
    use super::should_error_for_missing_subcommand;

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
}

#[cfg(all(test, feature = "server", feature = "desktop"))]
mod desktop_startup_tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn embedded_server_starts_and_serves_local_url() {
        let exporter = Exporter::default();
        let kibana_url = String::new();
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url)
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
        let (mut server, bound_addr) = Server::start([127, 0, 0, 1], 0, exporter, kibana_url)
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
