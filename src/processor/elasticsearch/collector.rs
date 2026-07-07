// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::collector::{ApiCollectOutcome, CollectOptions, CollectionResult, default_collect_archive_name};
use super::DiagnosticManifest;
use crate::{
    data::Product,
    exporter::ArchiveExporter,
    processor::RequestedApi,
    receiver::{ElasticCloudAdminRequestError, ElasticsearchRequestError, Receiver},
};
use eyre::{Result, WrapErr};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::processor::api::{ApiResolver, CollectConcurrencyPolicy, DiagnosticType, source_weight};
use futures::stream::{self, StreamExt};
use std::str::FromStr;
use std::time::Duration;

pub struct ElasticsearchCollector {
    receiver: Receiver,
    exporter: ArchiveExporter,
    options: CollectOptions,
}

impl ElasticsearchCollector {
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
            let collection_date = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let diag_type = DiagnosticType::from_str(&self.options.r#type)?;
            let apis =
                ApiResolver::resolve_es(&diag_type, self.options.include.as_ref(), self.options.exclude.as_ref())?;

            tracing::debug!("Resolved APIs for collection: {:?}", apis);
            let stack_version = self
                .receiver
                .source_context()
                .await?
                .version
                .map(|version| version.to_string());

            let mut result = CollectionResult {
                path: self.exporter.to_string(),
                success: 0,
                total: apis.len() + 1, // +1 for manifest
            };

            // Source weight governs collect concurrency (ADR-0017); the
            // weight -> concurrency mapping is tunable policy (ADR-0018).
            let policy = CollectConcurrencyPolicy::from_env();
            let mut heavy_apis = Vec::new();
            let mut light_apis = Vec::new();

            for api in apis {
                if policy.is_sequential(source_weight("elasticsearch", &api)) {
                    heavy_apis.push(api);
                } else {
                    light_apis.push(api);
                }
            }

            // Concurrent fetch for light-weight sources
            let mut light_stream = stream::iter(light_apis)
                .map(|api| async move { self.save_api_with_retry(&api).await })
                .buffer_unordered(policy.concurrent_pool);

            let mut requested_apis = BTreeMap::new();
            while let Some(res) = light_stream.next().await {
                result.success += res.saved;
                if let Some((name, requested_api)) = res.requested_api {
                    requested_apis.insert(name, requested_api);
                }
            }

            // Sequential fetch for Heavy APIs
            for api in heavy_apis {
                let res = self.save_api_with_retry(&api).await;
                result.success += res.saved;
                if let Some((name, requested_api)) = res.requested_api {
                    requested_apis.insert(name, requested_api);
                }
            }

            result.success += self
                .save_diagnostic_manifest(&requested_apis, stack_version, collection_date)
                .await?;

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

    async fn save_api_with_retry(&self, api: &str) -> ApiCollectOutcome {
        let max_duration = Duration::from_secs(300); // 5 minutes
        let start_time = std::time::Instant::now();
        let mut attempt = 1;
        let mut delay = Duration::from_secs(2);
        let mut retries = 0;
        let mut retried_response_time_ms = 0;
        let mut retried_response_size_bytes = 0;

        loop {
            let attempt_started = std::time::Instant::now();
            match self.save_api(api).await {
                Ok(mut success) => {
                    if let Some((_, requested_api)) = success.requested_api.as_mut() {
                        requested_api.retries = retries;
                        requested_api.response_time_ms += retried_response_time_ms;
                        requested_api.response_size_bytes += retried_response_size_bytes;
                    }
                    return success;
                }
                Err(e) => {
                    let (status, response_time_ms, response_size_bytes) = request_metrics(&e);
                    let response_time_ms = if response_time_ms == 0 {
                        attempt_started.elapsed().as_millis() as u64
                    } else {
                        response_time_ms
                    };
                    retried_response_time_ms += response_time_ms;
                    retried_response_size_bytes += response_size_bytes;
                    if !should_retry_elasticsearch_error(&e) {
                        tracing::warn!("Skipping non-retriable failure for {}: {}", api, e);
                        return ApiCollectOutcome::failed(
                            api,
                            status,
                            retries,
                            retried_response_time_ms,
                            retried_response_size_bytes,
                        );
                    }
                    if start_time.elapsed() > max_duration {
                        tracing::error!(
                            "Failed to save {} after {} attempts (5 min timeout): {}",
                            api,
                            attempt,
                            e
                        );
                        return ApiCollectOutcome::failed(
                            api,
                            status,
                            retries,
                            retried_response_time_ms,
                            retried_response_size_bytes,
                        );
                    }
                    tracing::warn!(
                        "Attempt {} failed for {}: {}. Retrying in {:?}...",
                        attempt,
                        api,
                        e,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    retries += 1;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(60));
                }
            }
        }
    }

    async fn save_api(&self, api: &str) -> Result<ApiCollectOutcome> {
        // Every source resolves through the registry by its canonical key
        // (ADR-0005) — the registry is the collect authority; the typed
        // structs exist only for processing.
        let response = self.save_raw(api).await?;

        match response {
            Some((requested_api, saved)) => Ok(ApiCollectOutcome::success(api, requested_api, 0, saved)),
            None => Ok(ApiCollectOutcome::skipped()),
        }
    }

    async fn save_raw(&self, name: &str) -> Result<Option<(RequestedApi, usize)>> {
        let source_conf = match crate::processor::diagnostic::data_source::get_source("elasticsearch", name, &[]) {
            Ok((_, conf)) => conf,
            Err(e) => {
                tracing::debug!("Skipping {} collection: {}", name, e);
                return Ok(None);
            }
        };

        let version = match &self.receiver {
            Receiver::Elasticsearch(r) => match r.get_version().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!("Cannot collect raw API without version: {}", e);
                    return Ok(None);
                }
            },
            Receiver::ElasticCloudAdmin(r) => match r.get_version().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!("Cannot collect raw API without version: {}", e);
                    return Ok(None);
                }
            },
            _ => return Ok(None),
        };

        let path = match source_conf.get_url(version) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("Skipping {} collection on version {}: {}", name, version, e);
                return Ok(None);
            }
        };

        let extension = source_conf.extension.as_deref().unwrap_or(".json");
        let response = match self.receiver.get_raw_response_by_path(&path, extension).await {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!("Failed to get raw API {}: {}", name, e);
                return Err(e);
            }
        };

        let file_path = PathBuf::from(source_conf.get_file_path(name));
        let filename = format!("{}", file_path.display());

        let requested_api = RequestedApi {
            status: response.status,
            retries: 0,
            response_time_ms: response.response_time_ms,
            response_size_bytes: response.response_size_bytes,
        };

        match self.exporter.save(file_path, response.body).await {
            Ok(()) => {
                tracing::info!("Saved {filename}");
                Ok(Some((requested_api, 1)))
            }
            Err(e) => {
                tracing::error!("Failed to save {filename}: {e}");
                Ok(Some((requested_api, 0)))
            }
        }
    }

    async fn save_diagnostic_manifest(
        &self,
        requested_apis: &BTreeMap<String, RequestedApi>,
        stack_version: Option<String>,
        collection_date: String,
    ) -> Result<usize> {
        let manifest = DiagnosticManifest::new(
            collection_date,
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some(self.options.r#type.clone()),
            Product::Elasticsearch,
            Some("elasticsearch_diagnostic".to_string()),
            Some("esdiag".to_string()),
            stack_version,
        )
        .with_identifiers(self.options.identifiers.clone())
        .with_requested_apis(requested_apis.clone());

        let path = PathBuf::from(DiagnosticManifest::FILENAME);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        tracing::info!("Saved {filename}");
        Ok(1)
    }
}

fn request_metrics(error: &eyre::Report) -> (Option<u16>, u64, u64) {
    if let Some(request_error) = error.downcast_ref::<ElasticsearchRequestError>() {
        return (
            Some(request_error.status.as_u16()),
            request_error.response_time_ms,
            request_error.response_size_bytes,
        );
    }
    if let Some(request_error) = error.downcast_ref::<ElasticCloudAdminRequestError>() {
        return (
            Some(request_error.status.as_u16()),
            request_error.response_time_ms,
            request_error.response_size_bytes,
        );
    }
    (None, 0, 0)
}

fn should_retry_elasticsearch_error(error: &eyre::Report) -> bool {
    if let Some(request_error) = error.downcast_ref::<ElasticsearchRequestError>() {
        return request_error.status.as_u16() == 429 || request_error.status.is_server_error();
    }
    if let Some(request_error) = error.downcast_ref::<ElasticCloudAdminRequestError>() {
        return request_error.status.as_u16() == 429 || request_error.status.is_server_error();
    }
    if let Some(request_error) = error.downcast_ref::<elasticsearch::Error>() {
        if let Some(status) = request_error.status_code() {
            return status.as_u16() == 429 || status.is_server_error();
        }
        if request_error.is_timeout() {
            return true;
        }
        return std::error::Error::source(request_error)
            .and_then(|source| source.downcast_ref::<reqwest::Error>())
            .is_some_and(is_retryable_reqwest_error);
    }
    if let Some(request_error) = error.downcast_ref::<reqwest::Error>() {
        return is_retryable_reqwest_error(request_error);
    }
    false
}

fn is_retryable_reqwest_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_body() || error.is_request()
}

#[cfg(test)]
mod tests {
    use super::*;
    use elasticsearch::http::StatusCode;

    #[test]
    fn retry_policy_skips_non_retriable_client_errors() {
        let unauthorized = eyre::Report::from(ElasticsearchRequestError {
            status: StatusCode::UNAUTHORIZED,
            body: "unauthorized".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let forbidden = eyre::Report::from(ElasticsearchRequestError {
            status: StatusCode::FORBIDDEN,
            body: "forbidden".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let not_found = eyre::Report::from(ElasticsearchRequestError {
            status: StatusCode::NOT_FOUND,
            body: "missing".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });

        assert!(!should_retry_elasticsearch_error(&unauthorized));
        assert!(!should_retry_elasticsearch_error(&forbidden));
        assert!(!should_retry_elasticsearch_error(&not_found));

        let cloud_admin_not_found = eyre::Report::from(ElasticCloudAdminRequestError {
            status: StatusCode::NOT_FOUND,
            body: "missing".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        assert!(!should_retry_elasticsearch_error(&cloud_admin_not_found));
    }

    #[test]
    fn retry_policy_retries_rate_limits_and_server_errors() {
        let rate_limited = eyre::Report::from(ElasticsearchRequestError {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: "slow down".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let server_error = eyre::Report::from(ElasticsearchRequestError {
            status: StatusCode::BAD_GATEWAY,
            body: "gateway".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let cloud_admin_server_error = eyre::Report::from(ElasticCloudAdminRequestError {
            status: StatusCode::BAD_GATEWAY,
            body: "gateway".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });

        assert!(should_retry_elasticsearch_error(&rate_limited));
        assert!(should_retry_elasticsearch_error(&server_error));
        assert!(should_retry_elasticsearch_error(&cloud_admin_server_error));
        assert!(!should_retry_elasticsearch_error(&eyre::eyre!("connection reset")));
    }
}
