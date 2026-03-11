// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::super::collector::{CollectOptions, CollectionResult};
use crate::{
    data::Product,
    exporter::ArchiveExporter,
    processor::{
        DiagnosticManifest,
        api::{ApiResolver, DiagnosticType, KibanaApi},
    },
    receiver::{KibanaReceiver, KibanaRequestError, Receiver},
};
use eyre::{Result, WrapErr, eyre};
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::{path::PathBuf, str::FromStr, time::Duration};

pub struct KibanaCollector {
    receiver: Receiver,
    exporter: ArchiveExporter,
    options: CollectOptions,
}

impl KibanaCollector {
    pub async fn new(
        receiver: Receiver,
        exporter: ArchiveExporter,
        options: CollectOptions,
    ) -> Result<Self> {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let collection_name = options
            .identifiers
            .filename
            .as_ref()
            .and_then(|name| std::path::Path::new(name).file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("api-diagnostics-{}", timestamp));
        Ok(Self {
            receiver,
            exporter: exporter.with_archive_name(&collection_name)?,
            options,
        })
    }

    pub async fn collect(&self) -> Result<CollectionResult> {
        let collect_result: Result<CollectionResult> = async {
            let diag_type = DiagnosticType::from_str(&self.options.r#type)?;
            let apis = ApiResolver::resolve_kb(
                &diag_type,
                self.options.include.as_ref(),
                self.options.exclude.as_ref(),
            )?;

            let mut result = CollectionResult {
                path: self.exporter.to_string(),
                success: 0,
                total: apis.len() + 1,
            };

            result.success += self.save_diagnostic_manifest(&apis).await?;

            let mut api_stream = stream::iter(apis)
                .map(|api| async move { self.save_api_with_retry(&api).await })
                .buffer_unordered(5);

            while let Some(res) = api_stream.next().await {
                result.success += res;
            }

            Ok(result)
        }
        .await;

        let finalize_result = self.exporter.finalize();

        match (collect_result, finalize_result) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(err), Ok(())) => Err(err),
            (Ok(_), Err(finalize_err)) => Err(finalize_err),
            (Err(err), Err(finalize_err)) => {
                Err(err).wrap_err(format!("Failed to finalize archive: {}", finalize_err))
            }
        }
    }

    async fn save_api_with_retry(&self, api: &KibanaApi) -> usize {
        let max_duration = Duration::from_secs(300);
        let start_time = std::time::Instant::now();
        let mut attempt = 1;
        let mut delay = Duration::from_secs(2);

        loop {
            match self.save_api(api).await {
                Ok(success) => return success,
                Err(e) => {
                    if !should_retry_kibana_error(&e) {
                        log::warn!("Skipping non-retriable failure for {}: {}", api.as_str(), e);
                        return 0;
                    }
                    if start_time.elapsed() > max_duration {
                        log::error!(
                            "Failed to save {} after {} attempts (5 min timeout): {}",
                            api.as_str(),
                            attempt,
                            e
                        );
                        return 0;
                    }
                    log::warn!(
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

    async fn save_api(&self, api: &KibanaApi) -> Result<usize> {
        let receiver = match &self.receiver {
            Receiver::Kibana(receiver) => receiver,
            _ => return Err(eyre!("KibanaCollector requires a Kibana receiver")),
        };
        let source_conf = match crate::processor::diagnostic::data_source::get_source(
            "kibana",
            api.as_str(),
            &[],
        ) {
            Ok((_, conf)) => conf,
            Err(e) => {
                log::debug!("Skipping {} collection: {}", api.as_str(), e);
                return Ok(0);
            }
        };

        let version = receiver.get_version().await?;
        let resolved = match source_conf.resolve_version(version) {
            Ok(resolved) => resolved,
            Err(e) => {
                log::debug!(
                    "Skipping {} collection on version {}: {}",
                    api.as_str(),
                    version,
                    e
                );
                return Ok(0);
            }
        };

        let scopes = if resolved.spaceaware {
            match receiver.get_spaces().await {
                Ok(spaces) if !spaces.is_empty() => spaces.clone(),
                Ok(_) => vec!["default".to_string()],
                Err(err) => {
                    log::warn!(
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
        for scope in scopes {
            let space = resolved.spaceaware.then_some(scope.as_str());
            saved += self
                .save_endpoint(
                    receiver,
                    api.as_str(),
                    source_conf,
                    &resolved.url,
                    space,
                    resolved.paginate.as_deref(),
                )
                .await?;
        }

        Ok(saved)
    }

    async fn save_endpoint(
        &self,
        receiver: &KibanaReceiver,
        api_name: &str,
        source_conf: &crate::processor::diagnostic::data_source::Source,
        base_url: &str,
        space: Option<&str>,
        paginate_field: Option<&str>,
    ) -> Result<usize> {
        let base_file_path = source_conf.get_file_path(api_name);
        let extension = source_conf.extension.as_deref().unwrap_or(".json");

        if let Some(paginate_field) = paginate_field {
            return self
                .save_paginated_endpoint(
                    receiver,
                    &base_file_path,
                    base_url,
                    space,
                    extension,
                    paginate_field,
                )
                .await;
        }

        let request_path = with_space_prefix(base_url, space);
        let content = receiver.get_raw_by_path(&request_path, extension).await?;
        self.save_content(&base_file_path, content, space, None)
            .await
    }

    async fn save_paginated_endpoint(
        &self,
        receiver: &KibanaReceiver,
        base_file_path: &str,
        base_url: &str,
        space: Option<&str>,
        extension: &str,
        paginate_field: &str,
    ) -> Result<usize> {
        const PAGE_SIZE: usize = 100;

        let mut page = 1;
        let mut total_pages = 1;
        let mut saved = 0;

        loop {
            let request_path = with_pagination_query(
                &with_space_prefix(base_url, space),
                paginate_field,
                page,
                PAGE_SIZE,
            );
            let content = receiver.get_raw_by_path(&request_path, extension).await?;
            total_pages =
                total_pages.max(parse_total_pages(&content, paginate_field, page).unwrap_or(page));

            let page_scope = (total_pages > 1).then_some(page);
            saved += self
                .save_content(base_file_path, content, space, page_scope)
                .await?;

            if page >= total_pages {
                break;
            }
            page += 1;
        }

        Ok(saved)
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
                log::info!("Saved {filename}");
                Ok(1)
            }
            Err(e) => {
                log::error!("Failed to save {filename}: {e}");
                Ok(0)
            }
        }
    }

    async fn save_diagnostic_manifest(&self, apis: &[KibanaApi]) -> Result<usize> {
        let version = match &self.receiver {
            Receiver::Kibana(receiver) => receiver.get_version().await?.to_string(),
            _ => return Err(eyre!("Kibana manifest requires a Kibana receiver")),
        };

        let collected_api_names: Vec<String> =
            apis.iter().map(|api| api.as_str().to_string()).collect();
        let manifest = DiagnosticManifest::new(
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
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
        .with_collected_apis(collected_api_names);

        let path = PathBuf::from(DiagnosticManifest::FILENAME);
        let filename = format!("{}", path.display());
        let content = serde_json::to_string_pretty(&manifest)?;
        self.exporter.save(path, content).await?;
        log::info!("Saved {filename}");
        Ok(1)
    }
}

fn with_space_prefix(path: &str, space: Option<&str>) -> String {
    match space {
        Some(space) if !space.is_empty() => format!("/s/{space}{path}"),
        _ => path.to_string(),
    }
}

fn with_pagination_query(
    path: &str,
    paginate_field: &str,
    page: usize,
    page_size: usize,
) -> String {
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
    if let Some(request_error) = error.downcast_ref::<KibanaRequestError>() {
        return request_error.status.as_u16() == 429 || request_error.status.is_server_error();
    }
    if let Some(request_error) = error.downcast_ref::<reqwest::Error>() {
        return request_error.is_connect() || request_error.is_timeout();
    }
    false
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
    fn retry_policy_skips_non_retriable_client_errors() {
        let error = eyre::Report::from(KibanaRequestError {
            status: StatusCode::FORBIDDEN,
            body: "forbidden".to_string(),
        });
        assert!(!should_retry_kibana_error(&error));
    }

    #[test]
    fn retry_policy_retries_server_errors_and_rate_limits() {
        let rate_limit = eyre::Report::from(KibanaRequestError {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: "slow down".to_string(),
        });
        let server_error = eyre::Report::from(KibanaRequestError {
            status: StatusCode::BAD_GATEWAY,
            body: "gateway".to_string(),
        });

        assert!(should_retry_kibana_error(&rate_limit));
        assert!(should_retry_kibana_error(&server_error));
    }
}
