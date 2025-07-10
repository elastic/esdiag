use clap::{Parser, Subcommand, builder::styling};
use esdiag::{
    client::{KnownHost, KnownHostBuilder},
    data::{Collector, Uri, diagnostic::Product},
    env::LOG_LEVEL,
    exporter::{DirectoryExporter, Exporter},
    processor::{Diagnostic, JobFailed, JobNew},
    receiver::Receiver,
    server::ApiServer,
    setup,
};
use eyre::{Result, eyre};
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
    Serve {
        /// The port to bind the server to
        #[arg(
            help = "The port to bind the server to",
            long,
            short,
            default_value = "3000"
        )]
        port: u16,
        /// Target to send processed diagnostic documents to
        #[arg(
            long_help = "Target to send the processed diagnostic documents to (known host, file, stdout, or env). Strings will be checked against the known hosts stored in `~/.esdiag/hosts.yml` and will fallback to a filename if not found. Use `-` for stdout. If nothing is provided, the output will try using the environment variables: ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, and ESDIAG_OUTPUT_PASSWORD."
        )]
        output: Option<String>,
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
    /// [DEPRECATED] Process, enrich and import a diagnostic into Elasticsearch
    Import {
        /// The target to write processed diagnostic data to
        #[arg(help = "Target to write processed diagnostic documents to (`-` for stdout)")]
        target: String,

        /// The source to read diagnostic data from
        #[arg(help = "Source to read diagnostic data from")]
        source: String,
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
    },
    /// Import assets (templates, ingest pipelines, etc.) to a known Elasticsearch host
    Setup {
        /// Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked.
        #[arg(
            help = "Known Elasticsearch host to import assets into; if omitted the ESDIAG_OUTPUT_URL, ESDIAG_OUTPUT_APIKEY, ESDIAG_OUTPUT_USERNAME, ESDIAG_OUTPUT_PASSWORD variables will be checked."
        )]
        host: Option<String>,
    },
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    let start_time = std::time::Instant::now();

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

    match run(cli).await {
        Ok(cmd) => {
            log::info!(
                "{cmd} complete in {:.3} seconds",
                start_time.elapsed().as_secs_f32()
            );
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
        Commands::Serve { port, output } => {
            log::info!("Starting ESDiag API server");

            let output_uri = output.and_then(|o| Uri::try_from(o).ok());
            let exporter = Exporter::try_from(output_uri)?;

            let mut server = ApiServer::new(port, exporter.to_string());

            let rx = match &server.rx {
                Some(rx) => rx.clone(),
                None => return Err(eyre!("Server rx error")),
            };

            // This will process uploaded diagnostics until a termination signal is received
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
                _ = async {
                    loop {
                        // Only receive the diagnostic - processing happens in a separate worker thread
                        let mut rx = rx.write().await;
                        match rx.recv().await {
                            Some((filename, bytes)) => {
                                let receiver = match Receiver::try_from(bytes) {
                                    Ok(receiver) => receiver,
                                    Err(e) => {
                                        let error = format!("Failed to create receiver: {}", e);
                                        log::error!("{}", error);
                                        server.job_record_failure(JobFailed::from(error)).await;
                                        continue;
                                    }
                                };
                                // Create job and push to queue for the worker thread to handle
                                match JobNew::new(filename, receiver).ready(exporter.clone()).await {
                                    Ok(job) => server.job_push(job.start()).await,
                                    Err(job) => server.job_record_failure(job).await,
                                };
                            }
                            None => {
                                let message = format!("Failed receiving archive bytes");
                                log::error!("{}", message);
                            }
                        }
                        log::debug!("Waiting for next diagnostic...");
                    }
                    #[allow(unreachable_code)]
                    Ok::<_, eyre::Report>(())
                } => {}
            }

            // Perform graceful shutdown of the server and worker thread
            server.shutdown().await;
            Ok("serve")
        }
        Commands::Collect { host, output } => {
            let known_host = Uri::try_from(host)?;
            let output = Uri::try_from(output)?;
            match known_host {
                Uri::KnownHost(_) => {
                    log::info!("Collecting diagnostic from {known_host}");
                    log::info!("Saving diagnostic to {output}");
                    let receiver = Receiver::try_from(known_host)?;
                    let exporter = DirectoryExporter::try_from(output)?;
                    let collector = Collector::try_new(receiver, exporter).await?;
                    collector.collect().await?;
                    Ok("collect")
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

            let valid_connection = match host.test().await {
                Ok((is_valid, message)) => {
                    match is_valid {
                        true => log::info!("Host {name}: {}", &message),
                        false => log::warn!("Host {name}: {}", &message),
                    }
                    is_valid
                }
                Err(e) => {
                    log::error!("Host connection: FAILED ❌ {}", &e);
                    log::debug!("{:?}", e);
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
        Commands::Process { input, output } => {
            let input_uri = Uri::try_from(input)?;
            let output_uri = output.and_then(|o| Uri::try_from(o).ok());

            log::info!("input: {}", input_uri);

            let receiver = Receiver::try_from(input_uri)?;
            let exporter = Exporter::try_from(output_uri)?;

            let manifest = receiver.try_get_manifest().await?;
            log::trace!("{}", serde_json::to_string(&manifest)?);

            let diagnostic = Diagnostic::try_new(manifest, receiver, exporter).await?;
            diagnostic.run().await.map(|_| "process")
        }
        Commands::Import { target, source } => {
            let input_uri = Uri::try_from(source)?;
            let output_uri = Uri::try_from(target)?;
            log::info!("input: {}", input_uri);
            log::info!("output: {}", output_uri);
            log::warn!("The `import` command is deprecated, please use `process` instead");

            let receiver = Receiver::try_from(input_uri)?;
            let exporter = Exporter::try_from(Some(output_uri))?;

            let manifest = receiver.try_get_manifest().await?;
            log::trace!("{}", serde_json::to_string(&manifest)?);

            let diagnostic = Diagnostic::try_new(manifest, receiver, exporter).await?;
            diagnostic.run().await.map(|_| "import")
        }
        Commands::Setup { host } => {
            let uri = match host {
                Some(host) => match Uri::try_from(host) {
                    Ok(uri) => Some(uri),
                    Err(e) => {
                        log::error!("Failed to convert host to Uri: {}", e);
                        return Err(e.into());
                    }
                },
                None => None,
            };
            let exporter = Exporter::try_from(uri)?;
            log::info!("Setting up Elasticsearch assets in {exporter}");
            setup::assets(exporter).await?;
            Ok("setup")
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
    let files = vec!["bulk_errors.ndjson", "diagnostic.json", "report.json"];
    for file in files {
        let file = last_run.join(file);
        log::debug!("Removing {}", &file.display());
        // Ignore "file not found" errors on delete
        let _ = std::fs::remove_file(file);
    }
    Ok(())
}
