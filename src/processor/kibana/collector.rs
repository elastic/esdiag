// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::collector::{CollectOptions, CollectionResult, default_collect_archive_name};
use crate::{
    data::Product,
    exporter::ArchiveExporter,
    processor::{
        DiagnosticManifest, RequestedApi,
        api::{ApiResolver, DiagnosticType, KibanaApi},
        collector::ApiCollectOutcome,
    },
    receiver::{KibanaReceiver, KibanaRequestError, Receiver},
};
use eyre::{Result, WrapErr, eyre};
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::{path::PathBuf, str::FromStr, time::Duration};

pub struct KibanaCollector {
    receiver: Receiver,
    exporter: ArchiveExporter,
    options: CollectOptions,
}

#[derive(Debug)]
struct KibanaPartialCollectError {
    source: eyre::Report,
    requested_api: Option<RequestedApi>,
    saved: usize,
}

impl KibanaPartialCollectError {
    fn new(source: eyre::Report, requested_api: Option<RequestedApi>, saved: usize) -> Self {
        Self {
            source,
            requested_api,
            saved,
        }
    }

    fn metrics(&self) -> (Option<u16>, u64, u64) {
        request_metrics(&self.source)
    }

    fn completed_metrics(&self) -> (u64, u64) {
        let mut response_time_ms = 0;
        let mut response_size_bytes = 0;
        if let Some(requested_api) = &self.requested_api {
            response_time_ms += requested_api.response_time_ms;
            response_size_bytes += requested_api.response_size_bytes;
        }
        if let Some(partial) = self.source.downcast_ref::<KibanaPartialCollectError>() {
            let (nested_time_ms, nested_size_bytes) = partial.completed_metrics();
            response_time_ms += nested_time_ms;
            response_size_bytes += nested_size_bytes;
        }
        (response_time_ms, response_size_bytes)
    }
}

impl fmt::Display for KibanaPartialCollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.source)
    }
}

impl std::error::Error for KibanaPartialCollectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl KibanaCollector {
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
                ApiResolver::resolve_kb(&diag_type, self.options.include.as_ref(), self.options.exclude.as_ref())?;

            let mut result = CollectionResult {
                path: self.exporter.to_string(),
                success: 0,
                total: apis.len() + 1,
            };

            let mut api_stream = stream::iter(apis)
                .map(|api| async move { self.save_api_with_retry(&api).await })
                .buffer_unordered(crate::client::KIBANA_REQUEST_CONCURRENCY);

            let mut requested_apis = BTreeMap::new();
            while let Some(res) = api_stream.next().await {
                result.success += res.saved;
                if let Some((name, requested_api)) = res.requested_api {
                    requested_apis.insert(name, requested_api);
                }
            }

            result.success += self.save_diagnostic_manifest(&requested_apis, collection_date).await?;

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

    async fn save_api_with_retry(&self, api: &KibanaApi) -> ApiCollectOutcome {
        let max_duration = Duration::from_secs(300);
        let start_time = std::time::Instant::now();
        let mut attempt = 1;
        let mut delay = Duration::from_secs(2);
        let mut retries = 0;
        let mut retried_response_time_ms = 0;
        let mut retried_response_size_bytes = 0;
        let mut retried_saved = 0;

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
                    let (completed_response_time_ms, completed_response_size_bytes) = e
                        .downcast_ref::<KibanaPartialCollectError>()
                        .map(KibanaPartialCollectError::completed_metrics)
                        .unwrap_or((0, 0));
                    let response_time_ms = fallback_response_time_ms(
                        response_time_ms,
                        attempt_started.elapsed().as_millis() as u64,
                        completed_response_time_ms,
                    );
                    if let Some(partial) = e.downcast_ref::<KibanaPartialCollectError>() {
                        retried_saved += partial.saved;
                    }
                    retried_response_time_ms += completed_response_time_ms + response_time_ms;
                    retried_response_size_bytes += completed_response_size_bytes + response_size_bytes;
                    if !should_retry_kibana_error(&e) {
                        tracing::warn!("Skipping non-retriable failure for {}: {}", api.as_str(), e);
                        return ApiCollectOutcome::failed_with_saved(
                            api.as_str(),
                            status,
                            retries,
                            retried_response_time_ms,
                            retried_response_size_bytes,
                            retried_saved,
                        );
                    }
                    if start_time.elapsed() > max_duration {
                        tracing::error!(
                            "Failed to save {} after {} attempts (5 min timeout): {}",
                            api.as_str(),
                            attempt,
                            e
                        );
                        return ApiCollectOutcome::failed_with_saved(
                            api.as_str(),
                            status,
                            retries,
                            retried_response_time_ms,
                            retried_response_size_bytes,
                            retried_saved,
                        );
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
                    retries += 1;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(60));
                }
            }
        }
    }

    async fn save_api(&self, api: &KibanaApi) -> Result<ApiCollectOutcome> {
        let receiver = match &self.receiver {
            Receiver::Kibana(receiver) => receiver,
            _ => return Err(eyre!("KibanaCollector requires a Kibana receiver")),
        };
        let source_conf = match crate::processor::diagnostic::data_source::get_source("kibana", api.as_str(), &[]) {
            Ok((_, conf)) => conf,
            Err(e) => {
                tracing::debug!("Skipping {} collection: {}", api.as_str(), e);
                return Ok(ApiCollectOutcome::skipped());
            }
        };

        let version = receiver.get_version().await?;
        let resolved = match source_conf.resolve_version(version) {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::debug!("Skipping {} collection on version {}: {}", api.as_str(), version, e);
                return Ok(ApiCollectOutcome::skipped());
            }
        };

        let scopes = if resolved.spaceaware {
            match receiver.get_spaces().await {
                Ok(spaces) if !spaces.is_empty() => spaces.clone(),
                Ok(_) => vec!["default".to_string()],
                Err(err) => {
                    tracing::warn!(
                        "Failed to resolve Kibana spaces for {}: {}. Falling back to default space.",
                        api.as_str(),
                        err
                    );
                    vec!["default".to_string()]
                }
            }
        } else {
            vec![String::new()]
        };

        let mut saved = 0;
        let mut requested_api: Option<RequestedApi> = None;
        for scope in scopes {
            let space = resolved.spaceaware.then_some(scope.as_str());
            let result = self
                .save_endpoint(
                    receiver,
                    api.as_str(),
                    source_conf,
                    &resolved.url,
                    space,
                    resolved.paginate.as_deref(),
                )
                .await;
            let (scope_requested_api, scope_saved) = match result {
                Ok(result) => result,
                Err(err) => {
                    let saved = saved
                        + err
                            .downcast_ref::<KibanaPartialCollectError>()
                            .map(|partial| partial.saved)
                            .unwrap_or(0);
                    return Err(KibanaPartialCollectError::new(err, requested_api, saved).into());
                }
            };
            saved += scope_saved;
            merge_requested_api(&mut requested_api, scope_requested_api);
        }

        match requested_api {
            Some(requested_api) => Ok(ApiCollectOutcome::success(api.as_str(), requested_api, 0, saved)),
            None => Ok(ApiCollectOutcome::skipped()),
        }
    }

    async fn save_endpoint(
        &self,
        receiver: &KibanaReceiver,
        api_name: &str,
        source_conf: &crate::processor::diagnostic::data_source::Source,
        base_url: &str,
        space: Option<&str>,
        paginate_field: Option<&str>,
    ) -> Result<(RequestedApi, usize)> {
        let base_file_path = source_conf.get_file_path(api_name);
        let extension = source_conf.extension.as_deref().unwrap_or(".json");

        if let Some(paginate_field) = paginate_field {
            return self
                .save_paginated_endpoint(receiver, &base_file_path, base_url, space, extension, paginate_field)
                .await;
        }

        let request_path = with_space_prefix(base_url, space);
        let response = receiver.get_raw_response_by_path(&request_path, extension).await?;
        let requested_api = RequestedApi {
            status: response.status,
            retries: 0,
            response_time_ms: response.response_time_ms,
            response_size_bytes: response.response_size_bytes,
        };
        let saved = self.save_content(&base_file_path, response.body, space, None).await?;
        Ok((requested_api, saved))
    }

    async fn save_paginated_endpoint(
        &self,
        receiver: &KibanaReceiver,
        base_file_path: &str,
        base_url: &str,
        space: Option<&str>,
        extension: &str,
        paginate_field: &str,
    ) -> Result<(RequestedApi, usize)> {
        const PAGE_SIZE: usize = 100;

        let mut page = 1;
        let mut total_pages = 1;
        let mut saved = 0;
        let mut requested_api: Option<RequestedApi> = None;

        loop {
            let request_path =
                with_pagination_query(&with_space_prefix(base_url, space), paginate_field, page, PAGE_SIZE);
            let response = match receiver.get_raw_response_by_path(&request_path, extension).await {
                Ok(response) => response,
                Err(err) => {
                    return Err(KibanaPartialCollectError::new(err, requested_api, saved).into());
                }
            };
            total_pages = total_pages.max(parse_total_pages(&response.body, paginate_field, page).unwrap_or(page));
            let page_requested_api = RequestedApi {
                status: response.status,
                retries: 0,
                response_time_ms: response.response_time_ms,
                response_size_bytes: response.response_size_bytes,
            };

            let page_scope = (total_pages > 1).then_some(page);
            saved += self
                .save_content(base_file_path, response.body, space, page_scope)
                .await?;
            merge_requested_api(&mut requested_api, page_requested_api);

            if page >= total_pages {
                break;
            }
            page += 1;
        }

        match requested_api {
            Some(requested_api) => Ok((requested_api, saved)),
            None => unreachable!("paginated endpoint should fetch at least one page"),
        }
    }

    async fn save_content(
        &self,
        base_file_path: &str,
        content: String,
        space: Option<&str>,
        page: Option<usize>,
    ) -> Result<usize> {
        let file_path = scoped_output_path(base_file_path, space, page);
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

    async fn save_diagnostic_manifest(
        &self,
        requested_apis: &BTreeMap<String, RequestedApi>,
        collection_date: String,
    ) -> Result<usize> {
        let version = match &self.receiver {
            Receiver::Kibana(receiver) => receiver.get_version().await?.to_string(),
            _ => return Err(eyre!("Kibana manifest requires a Kibana receiver")),
        };

        let manifest = DiagnosticManifest::new(
            collection_date,
            Some(format!("esdiag-{}", env!("CARGO_PKG_VERSION"))),
            None,
            None,
            Some(self.options.r#type.clone()),
            Product::Kibana,
            Some("kibana_diagnostic".to_string()),
            Some("esdiag".to_string()),
            Some(version),
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
    if let Some(partial_error) = find_error_source::<KibanaPartialCollectError>(error) {
        return partial_error.metrics();
    }
    if let Some(request_error) = find_error_source::<KibanaRequestError>(error) {
        return (
            Some(request_error.status.as_u16()),
            request_error.response_time_ms,
            request_error.response_size_bytes,
        );
    }
    (None, 0, 0)
}

fn merge_requested_api(target: &mut Option<RequestedApi>, next: RequestedApi) {
    match target {
        Some(current) => {
            current.status = next.status;
            current.response_time_ms += next.response_time_ms;
            current.response_size_bytes += next.response_size_bytes;
        }
        None => *target = Some(next),
    }
}

fn with_space_prefix(path: &str, space: Option<&str>) -> String {
    match space {
        Some(space) if !space.is_empty() => format!("/s/{space}{path}"),
        _ => path.to_string(),
    }
}

fn with_pagination_query(path: &str, paginate_field: &str, page: usize, page_size: usize) -> String {
    let mut request_path = path.to_string();
    let separator = if request_path.contains('?') { '&' } else { '?' };
    request_path.push(separator);
    request_path.push_str(&format!("page={page}"));

    if !request_path.contains(&format!("{paginate_field}=")) {
        request_path.push('&');
        request_path.push_str(&format!("{paginate_field}={page_size}"));
    }

    request_path
}

fn parse_total_pages(content: &str, paginate_field: &str, current_page: usize) -> Option<usize> {
    let value: Value = serde_json::from_str(content).ok()?;
    let total = value.get("total")?.as_u64()? as usize;
    let page = value
        .get("page")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(current_page);
    let per_page = lookup_page_size(&value, paginate_field)? as usize;
    if per_page == 0 {
        return None;
    }
    Some(std::cmp::max(page, total.div_ceil(per_page)))
}

fn lookup_page_size(value: &Value, paginate_field: &str) -> Option<u64> {
    value
        .get(paginate_field)
        .and_then(Value::as_u64)
        .or_else(|| value.get("per_page").and_then(Value::as_u64))
        .or_else(|| value.get("perPage").and_then(Value::as_u64))
}

fn scoped_output_path(base_file_path: &str, space: Option<&str>, page: Option<usize>) -> PathBuf {
    let mut path = PathBuf::new();
    if let Some(space) = space {
        path.push("spaces");
        path.push(sanitize_segment(space));
    }
    if let Some(page) = page {
        path.push("pages");
        path.push(format!("page-{:04}", page));
    }
    path.push(base_file_path);
    path
}

fn sanitize_segment(segment: &str) -> String {
    segment.replace(['/', '\\', ':'], "_")
}

fn should_retry_kibana_error(error: &eyre::Report) -> bool {
    if let Some(request_error) = find_error_source::<KibanaRequestError>(error) {
        return request_error.status.as_u16() == 408
            || request_error.status.as_u16() == 429
            || request_error.status.is_server_error();
    }
    if let Some(request_error) = find_error_source::<reqwest::Error>(error) {
        return is_retryable_reqwest_error(request_error);
    }
    if let Some(kibana_sync_error) = find_error_source::<kibana_sync::Error>(error) {
        return is_retryable_kibana_sync_error(kibana_sync_error);
    }
    false
}

fn find_error_source<T>(error: &eyre::Report) -> Option<&T>
where
    T: std::error::Error + 'static,
{
    error.chain().find_map(|source| source.downcast_ref::<T>())
}

fn is_retryable_kibana_sync_error(error: &kibana_sync::Error) -> bool {
    match error {
        kibana_sync::Error::Transport(request_error) => is_retryable_reqwest_error(request_error),
        kibana_sync::Error::Context { source, .. } => is_retryable_kibana_sync_error(source),
        _ => false,
    }
}

fn is_retryable_reqwest_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_body() || error.is_request()
}

fn fallback_response_time_ms(response_time_ms: u64, attempt_elapsed_ms: u64, completed_response_time_ms: u64) -> u64 {
    if response_time_ms == 0 {
        attempt_elapsed_ms.saturating_sub(completed_response_time_ms)
    } else {
        response_time_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn scoped_output_path_preserves_plain_file_layout() {
        let path = scoped_output_path("alerts/kibana_alerts.json", None, None);
        assert_eq!(path, PathBuf::from("alerts/kibana_alerts.json"));
    }

    #[test]
    fn scoped_output_path_adds_space_and_page_segments() {
        let path = scoped_output_path("alerts/kibana_alerts.json", Some("default"), Some(2));
        assert_eq!(
            path,
            PathBuf::from("spaces/default/pages/page-0002/alerts/kibana_alerts.json")
        );
    }

    #[test]
    fn pagination_query_appends_page_and_size() {
        let path = with_pagination_query("/api/alerts/_find?sort=asc", "per_page", 3, 100);
        assert_eq!(path, "/api/alerts/_find?sort=asc&page=3&per_page=100");
    }

    #[test]
    fn pagination_query_supports_page_size_style() {
        let path = with_pagination_query("/api/endpoint/metadata", "pageSize", 2, 50);
        assert_eq!(path, "/api/endpoint/metadata?page=2&pageSize=50");
    }

    #[test]
    fn parse_total_pages_supports_snake_case_page_size() {
        let body = r#"{"page":1,"per_page":100,"total":205,"data":[]}"#;
        assert_eq!(parse_total_pages(body, "per_page", 1), Some(3));
    }

    #[test]
    fn parse_total_pages_supports_camel_case_page_size() {
        let body = r#"{"page":1,"perPage":100,"total":150,"items":[]}"#;
        assert_eq!(parse_total_pages(body, "perPage", 1), Some(2));
    }

    #[test]
    fn merge_requested_api_aggregates_response_metrics() {
        let mut requested_api = Some(RequestedApi {
            status: Some(200),
            retries: 0,
            response_time_ms: 10,
            response_size_bytes: 20,
        });

        merge_requested_api(
            &mut requested_api,
            RequestedApi {
                status: Some(204),
                retries: 0,
                response_time_ms: 30,
                response_size_bytes: 40,
            },
        );

        assert_eq!(
            requested_api,
            Some(RequestedApi {
                status: Some(204),
                retries: 0,
                response_time_ms: 40,
                response_size_bytes: 60,
            })
        );
    }

    #[test]
    fn retry_policy_skips_non_retriable_client_errors() {
        let error = eyre::Report::from(KibanaRequestError {
            status: StatusCode::FORBIDDEN,
            body: "forbidden".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        assert!(!should_retry_kibana_error(&error));
    }

    #[test]
    fn retry_policy_retries_internal_server_errors() {
        let internal_server_error = eyre::Report::from(KibanaRequestError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: "internal".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });

        assert!(should_retry_kibana_error(&internal_server_error));
    }

    #[test]
    fn retry_policy_retries_request_timeouts_gateway_errors_and_rate_limits() {
        let request_timeout = eyre::Report::from(KibanaRequestError {
            status: StatusCode::REQUEST_TIMEOUT,
            body: "timeout".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let rate_limit = eyre::Report::from(KibanaRequestError {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: "slow down".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });
        let server_error = eyre::Report::from(KibanaRequestError {
            status: StatusCode::BAD_GATEWAY,
            body: "gateway".to_string(),
            response_time_ms: 0,
            response_size_bytes: 0,
        });

        assert!(should_retry_kibana_error(&request_timeout));
        assert!(should_retry_kibana_error(&rate_limit));
        assert!(should_retry_kibana_error(&server_error));
    }

    #[tokio::test]
    async fn retry_policy_retries_kibana_sync_transport_errors() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let close_connection = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.expect("accept connection");
            drop(socket);
        });
        let error = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .expect_err("request should fail");
        close_connection.await.expect("connection close task should finish");
        let sync_error = eyre::Report::from(kibana_sync::Error::Transport(error)).wrap_err("Failed to send request");

        assert!(should_retry_kibana_error(&sync_error));
    }

    #[test]
    fn partial_error_metrics_keep_final_error_separate_from_completed_pages() {
        let completed = RequestedApi {
            status: Some(200),
            retries: 0,
            response_time_ms: 25,
            response_size_bytes: 100,
        };
        let transport_error = eyre!("connection reset");
        let partial = eyre::Report::from(KibanaPartialCollectError::new(transport_error, Some(completed), 1));

        assert_eq!(request_metrics(&partial), (None, 0, 0));
        let partial = partial.downcast_ref::<KibanaPartialCollectError>().unwrap();
        assert_eq!(partial.completed_metrics(), (25, 100));
    }

    #[test]
    fn partial_error_elapsed_fallback_excludes_completed_request_time() {
        assert_eq!(fallback_response_time_ms(0, 40, 25), 15);
        assert_eq!(fallback_response_time_ms(0, 10, 25), 0);
        assert_eq!(fallback_response_time_ms(7, 40, 25), 7);
    }
}
