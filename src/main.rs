use clap::{Parser, Subcommand};
use color_eyre::eyre::{eyre, Result};
use esdiag::{
    client::Host,
    data::{diagnostic::Manifest, elasticsearch::Cluster, Uri},
    env::LOG_LEVEL,
    exporter::Exporter,
    processor::{diagnostic::DiagnosticProcessor, elasticsearch::ElasticsearchDiagnostic},
    receiver::Receiver,
    setup,
};
use url::Url;

// Define command line arguments
#[derive(Debug, Parser)]
#[command(name = "esdiag")]
#[command(about = "Elastic Stack Diagnostics (esdiag) - collect diagnostics and import into Elasticsearch", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// [NOT IMPLEMENTED] Collects diagnostics from a host's API endpoints
    Collect {
        /// The host to collect diagnostics from
        #[arg(help = "The host to collect diagnostics from")]
        host: String,
        /// The output directory to save the diagnostics to
        #[arg(help = "Theoutput to save the diagnostics to (file, directory)")]
        output: String,
    },
    /// Configure and test a remote host connection
    Host {
        /// A name to identify this host
        #[arg(help = "A name to identify this host")]
        name: String,
        /// Application of this host (elasticsearch, kibana, logstash, etc.)
        #[arg(help = "Application of this host (elasticsearch, kibana, logstash, etc.)")]
        app: Option<String>,
        /// A host URL to connect to
        #[arg(help = "A host URL to connect to")]
        url: Option<Url>,
        /// Authentication method to use (none, basic, apikey, etc.)
        #[arg(
            default_value = "none",
            help = "Authentication method to use (none, basic, apikey, etc.)",
            long
        )]
        auth: String,
        /// Accept invalid certificates
        #[arg(help = "Accept invalid certificates", long)]
        accept_invalid_certs: bool,
        /// ApiKey for authentication
        #[arg(help = "ApiKey, passed as http header ", long, short)]
        apikey: Option<String>,
        /// Elastic Cloud ID (optional)
        #[arg(help = "Elastic Cloud ID (optional)", long, short)]
        cloud_id: Option<String>,
        /// Username for authentication
        #[arg(help = "Username for authentication", long, short)]
        username: Option<String>,
        /// Password for authentication
        #[arg(help = "Password for authentication", long, short)]
        password: Option<String>,
        /// Save the host configuration
        #[arg(help = "Save the host configuration", long, short)]
        save: bool,
    },
    /// Process, enrich and import a diagnostic into Elasticsearch
    Import {
        /// The target to write processed diagnostic data to
        #[arg(help = "Target to write processed diagnostic documents to (`-` for stdout)")]
        target: String,

        /// The source to read diagnostic data from
        #[arg(help = "Source to read diagnostic data from")]
        source: String,
    },
    /// Setup required assets to visualize diagnostic imports
    Setup {
        /// Known host to setup assets in, only supports Elasticsearch or Kibana
        #[arg(help = "Host to setup assets in")]
        host: String,
    },
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", LOG_LEVEL);
    env_logger::Builder::from_env(env)
        .format_timestamp_millis()
        .init();
    color_eyre::install()?;

    std::panic::set_hook(Box::new(|panic| {
        // Log any panics as errors
        log::debug!("{:?}", panic);
        log::error!("{}", panic);
    }));

    match run().await {
        Ok(cmd) => {
            log::info!("Completed {cmd} successfully");
            Ok(())
        }
        Err(e) => {
            log::error!("{}", e);
            Err(eyre!(e))
        }
    }
}

async fn run() -> Result<&'static str> {
    // use clap to parse command line arguments
    let cli = Cli::parse();

    match &cli.command {
        Commands::Collect { host, output } => Err(eyre!(
            "Collect command not yet implemented! host: {}, output: {}",
            host,
            output
        )),
        Commands::Host {
            name,
            app,
            url,
            accept_invalid_certs,
            auth,
            apikey,
            cloud_id,
            username,
            password,
            save,
        } => {
            log::info!("Configuring host {name}");
            let host = match app.is_some() && url.is_some() {
                false => Host::get_known(&name).ok_or(eyre!(
                    "Host {name} not found, include `app` and `url` to setup a new host."
                ))?,
                true => Host::new(
                    url.clone().unwrap(),
                    app.clone().unwrap(),
                    auth.clone(),
                    accept_invalid_certs.clone(),
                    apikey.clone(),
                    cloud_id.clone(),
                    username.clone(),
                    password.clone(),
                ),
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

            if *save {
                let hostfile = host.save(name.to_string())?;
                log::info!("Host {name} successfully saved to {hostfile}");
            }
            match valid_connection {
                true => Ok("host"),
                false => Err(eyre!("Host connection failed")),
            }
        }
        Commands::Import { target, source } => {
            let output_uri = Uri::parse(target)?;
            let input_uri = Uri::parse(source)?;
            log::info!("input: {}", input_uri);
            log::info!("output: {}", output_uri);

            let receiver = Receiver::try_from(input_uri.clone())?;
            let exporter = Exporter::try_from(output_uri.clone())?;
            let manifest = if let Ok(manifest) = receiver.get::<Manifest>().await {
                manifest
            } else {
                // Fallback to building a manifest if one doesn't exist
                let version = receiver.get::<Cluster>().await?;
                Manifest::try_from(version)?
            };
            log::trace!("{}", serde_json::to_string(&manifest).unwrap());
            let diagnostic_processor =
                ElasticsearchDiagnostic::new(manifest, receiver, exporter).await?;
            let doc_count = diagnostic_processor.run().await?;
            log::info!("Exported {} documents", doc_count);
            Ok("import")
        }
        Commands::Setup { host } => {
            log::info!("Setting up Elasticsearch assets in {host}");
            let uri = Uri::parse(host)?;
            let exporter = Exporter::try_from(uri)?;
            setup::assets(exporter).await?;
            Ok("setup")
        }
    }
}
