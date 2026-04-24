// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{node::Node, node_stats::NodeStats, version::Version};
use crate::{
    data::Product,
    exporter::ArchiveExporter,
    processor::{
        DataSource, DiagnosticManifest, SourceContext,
        api::{ApiResolver, ApiWeight, DiagnosticType, LogstashApi},
        collector::{CollectOptions, CollectionResult, default_collect_archive_name},
    },
    receiver::{LogstashRequestError, Receiver},
};
use eyre::{Result, WrapErr};
use futures::stream::{self, StreamExt};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

pub struct LogstashCollector {
    receiver: Receiver,
    exporter: ArchiveExporter,
    options: CollectOptions,
}

impl LogstashCollector {
    pub async fn new(receiver: Receiver, exporter: ArchiveExporter, options: CollectOptions) -> Result<Self> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let collection_name = options
            .identifiers
            .filename
            .as_ref()
            .and_then(|name| std::path::Path::new(name).file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| default_collect_archive_name(&options.product, &timestamp));
        Ok(Self {
            receiver,
            exporter: exporter.with_archive_name(&collection_name)?,
            options,
        })
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let collect_result: Result<CollectionResult> = async {
            let diag_type = DiagnosticType::from_str(&self.options.r#type)?;
            let apis =
                ApiResolver::resolve_ls(&diag_type, self.options.include.as_ref(), self.options.exclude.as_ref())?;

            let api_names: Vec<&str> = apis.iter().map(|a| a.as_str()).collect();
            tracing::debug!("Resolved Logstash APIs for collection: {:?}", api_names);

            let mut result = CollectionResult {
                path: self.exporter.to_string(),
                success: 0,
                total: apis.len() + 1,
            };

            let mut heavy_apis = Vec::new();
            let mut light_apis = Vec::new();

            result.success += self.save_diagnostic_manifest(&apis).await?;

            for api in apis {
                if api.weight() == ApiWeight::Heavy {
                    heavy_apis.push(api);
                } else {
                    light_apis.push(api);
                }
            }

            let mut light_stream = stream::iter(light_apis)
                .map(|api| async move { self.save_api_with_retry(&api).await })
                .buffer_unordered(5);

            while let Some(res) = light_stream.next().await {
                result.success += res;
            }

            for api in heavy_apis {
                result.success += self.save_api_with_retry(&api).await;
            }

            Ok(result)
        }
        .await;

        let finalize_result = self.exporter.finalize();

        match (collect_result, finalize_result) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(err), Ok(())) => Err(err),
            (Ok(_), Err(finalize_err)) => Err(finalize_err),
            (Err(err), Err(finalize_err)) => Err(err).wrap_err(format!("Failed to finalize archive: {}", finalize_err)),
        }
    }

    async fn save_api_with_retry(&self, api: &LogstashApi) -> usize {
        let max_duration = Duration::from_secs(300);
        let start_time = std::time::Instant::now();
        let mut attempt = 1;
        let mut delay = Duration::from_secs(2);

        loop {
            match self.save_api(api).await {
                Ok(success) => return success,
                Err(e) => {
                    if !should_retry_logstash_error(&e) {
                        tracing::warn!(
                            "Skipping non-retriable authentication failure for {}: {}",
                            api.as_str(),
                            e
                        );
                        return 0;
                    }
                    if start_time.elapsed() > max_duration {
                        tracing::error!(
                            "Failed to save {} after {} attempts (5 min timeout): {}",
                            api.as_str(),
                            attempt,
                            e
                        );
                        return 0;
                    }
                    tracing::warn!(
                        "Attempt {} failed for {}: {}. Retrying in {:?}...",
                        attempt,
                        api.as_str(),
                        e,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(60));
                }
            }
        }
    }

    async fn save_api(&self, api: &LogstashApi) -> Result<usize> {
        match api {
            LogstashApi::Node => self.save::<Node>().await,
            LogstashApi::NodeStats => self.save::<NodeStats>().await,
            LogstashApi::Raw(name, _) => self.save_raw(name).await,
        }
    }

    async fn save_raw(&self, name: &str) -> Result<usize> {
        let source_conf = match crate::processor::diagnostic::data_source::get_source("logstash", name, &[]) {
            Ok((_, conf)) => conf,
            Err(e) => {
                tracing::debug!("Skipping {} collection: {}", name, e);
                return Ok(0);
            }
        };

        let version = match &self.receiver {
            Receiver::Logstash(r) => match r.get_version().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!("Cannot collect raw API without version: {}", e);
                    return Ok(0);
                }
            },
            _ => return Ok(0),
        };

        let path = match source_conf.get_url(version) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("Skipping {} collection on version {}: {}", name, version, e);
                return Ok(0);
            }
        };

        let extension = source_conf.extension.as_deref().unwrap_or(".json");
        let content = match self.receiver.get_raw_by_path(&path, extension).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to get raw API {}: {}", name, e);
                return Err(e);
            }
        };

        let file_path = PathBuf::from(source_conf.get_file_path(name));
        let filename = format!("{}", file_path.display());

        match self.exporter.save(file_path, content).await {
            Ok(()) => {
                tracing::info!("Saved {filename}");
                Ok(1)
            }
            Err(e) => {
                tracing::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save<T>(&self) -> Result<usize>
    where
        T: DataSource,
    {
        let content = match self.receiver.get_raw::<T>().await {
            Ok(c) => c,
            Err(e) => {
                if let Some(ds_err) = e.downcast_ref::<crate::processor::diagnostic::data_source::DataSourceError>() {
                    tracing::debug!("Skipping {} collection: {}", T::name(), ds_err);
                    return Ok(0);
                }
                return Err(e);
            }
        };

        let ctx = SourceContext::new("logstash", None);
        let path = PathBuf::from(T::resolve_source_file_path(&ctx)?);
        let filename = format!("{}", path.display());
        match self.exporter.save(path, content).await {
            Ok(()) => {
                tracing::info!("Saved {filename}");
                Ok(1)
            }
            Err(e) => {
                tracing::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save_diagnostic_manifest(&self, apis: &[LogstashApi]) -> Result<usize> {
        let version = self.receiver.get::<Version>().await?;
        let parsed_version = semver::Version::parse(&version.version)?;
        let collected_api_names: Vec<String> = apis
            .iter()
            .filter_map(|api| match api {
                LogstashApi::Node => Some(api.as_str().to_string()),
                LogstashApi::NodeStats => Some(api.as_str().to_string()),
                LogstashApi::Raw(name, _) => {
                    match crate::processor::diagnostic::data_source::get_source("logstash", name, &[]) {
                        Ok((_, source_conf)) => {
                            if source_conf.get_url(&parsed_version).is_ok() {
                                Some(api.as_str().to_string())
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                }
            })
            .collect();

        let manifest = DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some(self.options.r#type.clone()),
            Product::Logstash,
            Some("logstash_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(version.version),
        )
        .with_identifiers(self.options.identifiers.clone())
        .with_collected_apis(collected_api_names);

        let path = PathBuf::from(DiagnosticManifest::FILENAME);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        tracing::info!("Saved {filename}");
        Ok(1)
    }
}

fn should_retry_logstash_error(error: &eyre::Report) -> bool {
    if let Some(request_error) = error.downcast_ref::<LogstashRequestError>() {
        return request_error.status != reqwest::StatusCode::UNAUTHORIZED
            && request_error.status != reqwest::StatusCode::FORBIDDEN;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn retry_policy_skips_authentication_errors() {
        let unauthorized = eyre::Report::from(LogstashRequestError {
            status: StatusCode::UNAUTHORIZED,
            body: "unauthorized".to_string(),
        });
        let forbidden = eyre::Report::from(LogstashRequestError {
            status: StatusCode::FORBIDDEN,
            body: "forbidden".to_string(),
        });

        assert!(!should_retry_logstash_error(&unauthorized));
        assert!(!should_retry_logstash_error(&forbidden));
    }

    #[test]
    fn retry_policy_retries_non_auth_failures() {
        let rate_limited = eyre::Report::from(LogstashRequestError {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: "slow down".to_string(),
        });
        let transport = eyre::eyre!("connection reset");

        assert!(should_retry_logstash_error(&rate_limited));
        assert!(should_retry_logstash_error(&transport));
    }
}
