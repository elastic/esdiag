use clap::{Parser, Subcommand};
use esdiag::{
    data::diagnostic::Manifest, env::LOG_LEVEL, exporter::Output, host::Host, processor,
    receiver::Input, setup, uri::Uri,
};
use std::{panic, str::FromStr};
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
async fn main() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", LOG_LEVEL);
    env_logger::Builder::from_env(env)
        .format_timestamp_millis()
        //.target(env_logger::Target::Stdout)
        .init();

    panic::set_hook(Box::new(|panic| {
        // Use the error level to log the panic
        log::debug!("{:?}", panic);
        log::error!("{}", panic);
    }));

    // use clap to parse command line arguments
    let cli = Cli::parse();
    log::debug!("{:?}", cli);

    match &cli.command {
        Commands::Collect { host, output } => {
            unimplemented!(
                "Collect command not yet implemented! host: {}, output: {}",
                host,
                output
            );
        }
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
                false => match Host::get_known(&name) {
                    Some(host) => host,
                    None => {
                        log::error!(
                            "Application and URL must be specified for new host configurations"
                        );
                        return;
                    }
                },
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

            if valid_connection && *save {
                match host.save(name.to_string()) {
                    Ok(_) => {
                        let hosts_file = Host::get_hosts_path();
                        log::info!(
                            "Host '{name}' saved to {}",
                            hosts_file.to_str().expect("Failed to get hosts file path")
                        );
                    }
                    Err(e) => log::error!("Failed to save host configuration: {}", e),
                }
            }
        }
        Commands::Import { target, source } => {
            let output_uri = match Uri::parse(target) {
                Ok(uri) => uri,
                Err(e) => {
                    log::debug!("Invalid target: {:?}", e);
                    panic!("Invalid ouput: {}", target);
                }
            };
            let input_uri = match Uri::parse(source) {
                Ok(uri) => uri,
                Err(e) => {
                    log::debug!("Invalid source: {:?}", e);
                    panic!("Invalid input: {}", source);
                }
            };
            log::info!("input: {}", input_uri);
            log::info!("output: {}", output_uri);

            let manifest = Manifest::from_uri(&input_uri).expect("Failed to parse manifest");
            let input = Input::new(input_uri, manifest);
            let output = Output::from_uri(output_uri);
            processor::diagnostic::import(input, output)
                .await
                .expect("Failed to import diagnostics");
        }
        Commands::Setup { host } => {
            log::info!("Setting up Elasticsearch assets in {host}");
            let host = Host::from_str(host).expect("Failed to parse host for setup");
            let output = Output::from_host(host);
            match setup::assets(output).await {
                Ok(_) => log::info!("Assets setup complete"),
                Err(e) => log::error!("Failed to setup assets: {}", e),
            };
        }
    }
}
