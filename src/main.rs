mod host;
mod input;
mod output;
mod processor;
mod setup;
mod uri;
use async_std::task::{self, JoinHandle};
use clap::{Parser, Subcommand};
use futures::future::join_all;
use futures::stream::FuturesUnordered;
use host::Host;
use input::Input;
use log;
use output::Output;
use processor::Processor;
use serde_json::Value;
use std::{collections::HashMap, panic, str::FromStr, sync::Arc, thread};
use uri::Uri;
use url::Url;

use crate::output::file;

// Define command line arguments
#[derive(Parser)]
#[command(name = "esdiag")]
#[command(about = "Elastic Stack Diagnostics (esdiag) - collect diagnostics and import into Elasticsearch", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
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
    /// Process, enrich and import a diagnostic into Elasticsearch
    Import {
        /// The target to write processed diagnostic data to
        #[arg(help = "Target to write processed diagnostic documents to (`-` for stdout)")]
        target: String,

        /// The source to read diagnostic data from
        #[arg(help = "Source to read diagnostic data from")]
        source: String,

        /// Pretty print JSON outputs, files and stdout only (default: false)
        #[arg(default_value = "false", help = "Pretty print JSON", long, short)]
        pretty: bool,
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
    /// Setup required assets to visualize diagnostic imports
    Setup {
        /// Known host to setup assets in, only supports Elasticsearch or Kibana
        #[arg(help = "Host to setup assets in")]
        host: String,
    },
}

#[async_std::main]
async fn main() {
    let env = env_logger::Env::default().filter_or("LOG_LEVEL", "info");
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

    match &cli.command {
        Commands::Collect { host, output } => {
            unimplemented!(
                "Collect command not yet implemented! host: {}, output: {}",
                host,
                output
            );
            //log::info!("Collecting diagnostics from {}", host);
            //collect_diagnostics(host, output).await;
        }
        Commands::Import {
            target,
            source,
            pretty,
        } => {
            let output_uri = match uri::classify(target) {
                Ok(uri) => uri,
                Err(e) => {
                    log::debug!("Invalid target: {}", e);
                    panic!("Invalid ouput: {}", target);
                }
            };
            let input_uri = match uri::classify(source) {
                Ok(uri) => uri,
                Err(e) => {
                    log::debug!("Invalid source: {}", e);
                    panic!("Invalid input: {}", source);
                }
            };
            log::info!("input: {:?}", input_uri);
            log::info!("output: {:?}", output_uri);

            let manifest = match &input_uri {
                Uri::Directory(dir) => match input::file::parse_manifest(&dir) {
                    Ok(manifest) => manifest,
                    Err(e) => panic!("Failed to parse manifest - {}", e),
                },
                _ => panic!("Diagnostic manifest can only load from a directory input"),
            };
            let input = Input::new(input_uri, manifest);
            let output = Output::from_uri(output_uri, *pretty);
            import_diagnostics(input, output).await;
        }
        Commands::Host {
            name,
            app,
            url,
            auth,
            apikey,
            cloud_id,
            username,
            password,
            save,
        } => {
            log::info!("Configuring host {name}");
            let host = match Host::get_known(name) {
                Some(host) => host,
                None => match app {
                    None => {
                        log::error!("Application must be specified for new host configurations");
                        return;
                    }
                    Some(app) => Host::new(
                        url.clone().unwrap(),
                        app.clone(),
                        auth.clone(),
                        apikey.clone(),
                        cloud_id.clone(),
                        username.clone(),
                        password.clone(),
                    ),
                },
            };

            let valid_connection = match host.test().await {
                Ok(response) => {
                    log::info!("Host connection {name}: {}", response.status());
                    true
                }
                Err(e) => {
                    log::error!("Host connection: FAILED {:?}", e);
                    false
                }
            };

            if valid_connection && *save {
                match host.save(name.to_string()) {
                    Ok(_) => {
                        let hosts_file = host::get_hosts_path();
                        log::info!(
                            "Host '{name}' saved to {}",
                            hosts_file.to_str().expect("Failed to get hosts file path")
                        );
                    }
                    Err(e) => log::error!("Failed to save host configuration: {}", e),
                }
            }
        }
        Commands::Setup { host } => {
            log::info!("Setting up Elasticsearch assets in {host}");
            let host = Host::from_str(host).unwrap();
            let output = Output::from_host(host);
            match setup::assets(output).await {
                Ok(_) => log::info!("Assets setup complete"),
                Err(e) => log::error!("Failed to setup assets: {}", e),
            };
        }
    }
}

async fn import_diagnostics(input: Input, output: Output) {
    let metadata_content: HashMap<String, String> = input
        .dataset
        .metadata
        .iter()
        .filter_map(|dataset| match input.load_string(dataset) {
            Some(data) => Some((dataset.to_string(), data)),
            None => {
                log::warn!("Failed to load metadata for {}", dataset.to_string());
                None
            }
        })
        .collect();

    log::debug!("metadata_content keys: {:?}", metadata_content.keys());

    let mut processor = Processor::new(&input.manifest, metadata_content);

    let futures = FuturesUnordered::new();
    let input = Arc::new(input);
    let output = Arc::new(output);

    for lookup in &input.dataset.lookup {
        match input.load_string(&lookup) {
            Some(data) => {
                if let Some(docs) = processor.enrich_lookup(&lookup, data) {
                    let output: Arc<Output> = Arc::clone(&output);
                    let future = async move {
                        output.send(docs).await;
                    };
                    futures.push(task::spawn(future));
                }
            }
            None => {
                log::debug!("No docs for lookup {}", lookup.to_string());
            }
        }
    }

    // If debug logging, save metadata to file
    if log::log_enabled!(log::Level::Debug) {
        for (input, data) in processor.metadata.to_hashmap() {
            file::write_ndjson_if_debug(&input, data, "metadata.ndjson").ok();
        }
    }

    let data_sets = input.dataset.data.clone();
    let processor = Arc::new(processor);

    // Process each data set in parallel and push the resulting futures into `futures`
    for data_set in data_sets {
        let input: Arc<Input> = Arc::clone(&input);
        let processor: Arc<Processor> = Arc::clone(&processor);
        let output: Arc<Output> = Arc::clone(&output);
        let data: Value = input.load_value(&data_set);

        let future = async move {
            output.send(processor.enrich(&data_set, data)).await;
        };

        futures.push(task::spawn(future));
    }

    // Await all futures to complete before terminating the program
    join_all(futures).await;
}
