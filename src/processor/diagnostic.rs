use super::Processor;
use crate::exporter::{self, Output};
use crate::receiver::Input;
use color_eyre::Result;
use futures::{future::join_all, stream::FuturesUnordered};
use std::{collections::HashMap, sync::Arc};
use tokio::task;

pub async fn import(input: Input, output: Output) -> Result<()> {
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
        let lookup_name = lookup.to_string();

        match input.load_string(&lookup) {
            Some(data) => {
                if let Some(docs) = processor.enrich_lookup(&lookup, data) {
                    let output: Arc<Output> = Arc::clone(&output);
                    let future = task::spawn(async move {
                        let count = output.send(docs).await.unwrap_or_else(|e| {
                            log::error!("Failed to send data to output: {}", e);
                            0
                        });
                        log::info!(
                            "Sent {} docs for {} to {}",
                            &count,
                            lookup_name,
                            output.target,
                        );
                        count
                    });
                    futures.push(future);
                }
            }
            None => {
                log::info!("No docs for lookup: {}", lookup.to_string());
            }
        }
    }

    // If debug logging, save metadata to file
    exporter::file::debug_save("metadata.json", &processor.metadata)?;

    let data_sets = input.dataset.data.clone();
    let processor = Arc::new(processor);

    // Process each data set in parallel and push the resulting futures into `futures`
    for data_set in data_sets {
        let name = data_set.to_string();
        let input: Arc<Input> = Arc::clone(&input);
        let processor: Arc<Processor> = Arc::clone(&processor);
        let output: Arc<Output> = Arc::clone(&output);

        let future = task::spawn(async move {
            let data = task::spawn_blocking(move || match input.load_string(&data_set) {
                Some(string) => processor.enrich(&data_set, string),
                None => {
                    log::warn!("Failed to load data for {}", data_set.to_string());
                    Vec::new()
                }
            })
            .await
            .unwrap_or_else(|e| {
                log::error!("Failed to enrich data: {}", e);
                Vec::new()
            });

            let count = output.send(data).await.unwrap_or_else(|e| {
                log::error!("Failed to send data to output: {}", e);
                0
            });
            log::info!("Sent {} docs for {} to {}", count, name, output.target,);
            count
        });
        futures.push(future);
    }

    // Await all futures to complete, and sum the total count of docs processed
    let doc_count = join_all(futures).await;

    log::debug!("{}", input.dataset,);
    log::info!(
        "Import complete! Sent {} docs from {} sources for diagnostic: {}",
        doc_count.into_iter().map(|x| x.unwrap_or(0)).sum::<usize>(),
        input.dataset.len(),
        &processor.metadata.diagnostic.uuid
    );
    Ok(())
}
