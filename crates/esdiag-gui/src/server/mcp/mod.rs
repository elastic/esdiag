// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The `/mcp` Streamable HTTP endpoint.

use super::{RuntimeMode, ServerState};
use crate::{exporter::Exporter, job::admission::JobAdmissionError};
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header, uri::Authority},
    middleware::Next,
    response::{IntoResponse, Response},
};
use esdiag_core::mcp::{
    EsdiagMcp, McpPolicy,
    host::{AdmissionGuard, AdmissionRecord, McpHost, RequestIdentity},
};
use eyre::{Result, eyre};
use futures::future::BoxFuture;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use std::{net::Ipv4Addr, sync::Arc};
use tokio_util::sync::CancellationToken;

const ALLOWED_HOSTS_ENV: &str = "ESDIAG_MCP_ALLOWED_HOSTS";
const DEFAULT_ALLOWED_HOSTS: &str = "localhost,127.0.0.1,::1";

/// The `Host` authorities `/mcp` accepts. An entry without a port matches any port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AllowedHosts {
    entries: Vec<AllowedHost>,
    is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AllowedHost {
    host: String,
    port: Option<u16>,
}

impl AllowedHosts {
    /// `--mcp-allowed-hosts`, then `ESDIAG_MCP_ALLOWED_HOSTS`, then loopback.
    pub(crate) fn resolve(cli: Option<&str>) -> Result<Self> {
        match cli {
            Some(value) => Self::parse(value, false),
            None => match std::env::var(ALLOWED_HOSTS_ENV) {
                Ok(value) => Self::parse(&value, false),
                Err(std::env::VarError::NotPresent) => Self::parse(DEFAULT_ALLOWED_HOSTS, true),
                Err(err) => Err(eyre!("{ALLOWED_HOSTS_ENV}: {err}")),
            },
        }
    }

    fn parse(value: &str, is_default: bool) -> Result<Self> {
        let entries = value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(AllowedHost::parse)
            .collect::<Result<Vec<_>>>()?;
        if entries.is_empty() {
            return Err(eyre!("MCP allowed hosts must list at least one host"));
        }
        Ok(Self { entries, is_default })
    }

    /// Entries in the `host[:port]` form rmcp's `allowed_hosts` accepts.
    fn authorities(&self) -> Vec<String> {
        self.entries.iter().map(AllowedHost::authority).collect()
    }

    /// `http` and `https` origins for each entry; a portless entry allows any port.
    fn origins(&self) -> Vec<String> {
        self.entries
            .iter()
            .flat_map(|entry| {
                let host = entry.url_host();
                let port = entry.port.map_or_else(|| "*".to_string(), |port| port.to_string());
                ["http", "https"].map(|scheme| format!("{scheme}://{host}:{port}"))
            })
            .collect()
    }

    fn allows_host(&self, value: &str) -> bool {
        let Ok(authority) = Authority::try_from(value) else {
            return false;
        };
        self.allows(&authority, authority.port_u16())
    }

    /// Whether an entry matches `authority`'s host and, when the entry names
    /// a port, `port`.
    fn allows(&self, authority: &Authority, port: Option<u16>) -> bool {
        let host = normalize_host(authority.host());
        self.entries
            .iter()
            .any(|entry| entry.host == host && entry.port.is_none_or(|allowed| port == Some(allowed)))
    }

    fn allows_origin(&self, value: &str) -> bool {
        let Ok(uri) = value.parse::<axum::http::Uri>() else {
            return false;
        };
        let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
            return false;
        };
        let default_port = match scheme {
            "http" => 80,
            "https" => 443,
            _ => return false,
        };
        self.allows(authority, Some(authority.port_u16().unwrap_or(default_port)))
    }

    /// Remote clients are rejected when the server binds beyond loopback but
    /// only loopback hosts are allowed.
    pub(crate) fn warn_if_unreachable(&self, bind_addr: [u8; 4]) {
        if self.is_default && !Ipv4Addr::from(bind_addr).is_loopback() {
            tracing::warn!(
                "/mcp allows only loopback Host headers while the server binds {}; set --mcp-allowed-hosts or {ALLOWED_HOSTS_ENV} for remote clients",
                Ipv4Addr::from(bind_addr)
            );
        }
    }
}

impl AllowedHost {
    fn parse(entry: &str) -> Result<Self> {
        let invalid = || eyre!("Invalid MCP allowed host '{entry}', expected a host name or host:port");
        let (host, port) = match Authority::try_from(entry) {
            Ok(authority) => {
                let after_host = entry.strip_prefix(authority.host()).unwrap_or_default();
                if !after_host.is_empty() && authority.port_u16().is_none() {
                    return Err(invalid());
                }
                (authority.host().to_string(), authority.port_u16())
            }
            Err(_) if entry.parse::<std::net::Ipv6Addr>().is_ok() => (entry.to_string(), None),
            Err(_) => return Err(invalid()),
        };
        let host = normalize_host(&host);
        if host.is_empty() || entry.contains(['/', '@']) {
            return Err(invalid());
        }
        Ok(Self { host, port })
    }

    fn url_host(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }

    fn authority(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.url_host()),
            None => self.host.clone(),
        }
    }
}

fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase()
}

/// Rejects `/mcp` requests whose `Host`, or `Origin` when present, is not
/// allowed. Layered outside request authentication, so a disallowed host gets
/// 403 whatever the authentication provider would answer.
pub(crate) async fn host_guard(State(allowed): State<Arc<AllowedHosts>>, request: Request, next: Next) -> Response {
    if request.uri().path() != PATH {
        return next.run(request).await;
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| request.uri().authority().map(ToString::to_string));
    if !host.as_deref().is_some_and(|host| allowed.allows_host(host)) {
        tracing::warn!("Rejected /mcp request with disallowed Host header");
        return (StatusCode::FORBIDDEN, "Forbidden: Host header is not allowed").into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN)
        && !origin.to_str().is_ok_and(|origin| allowed.allows_origin(origin))
    {
        tracing::warn!("Rejected /mcp request with disallowed Origin header");
        return (StatusCode::FORBIDDEN, "Forbidden: Origin header is not allowed").into_response();
    }
    next.run(request).await
}

/// The web server as the MCP host: request identity, concurrency caps, and
/// the startup exporter.
pub(crate) struct GuiMcpHost(pub(crate) Arc<ServerState>);

impl McpHost for GuiMcpHost {
    fn identity(&self, headers: &HeaderMap) -> Result<Option<RequestIdentity>> {
        let identity = self.0.resolve_identity(headers)?;
        Ok(Some(RequestIdentity {
            user: identity.user,
            account: identity.account,
        }))
    }

    fn admit(&self, owner: String) -> BoxFuture<'_, std::result::Result<AdmissionGuard, JobAdmissionError>> {
        Box::pin(async move {
            if let Err(err) = self.0.record_job_started(&owner).await {
                self.0.record_job_rejected().await;
                return Err(err);
            }
            let state = Arc::clone(&self.0);
            Ok(AdmissionGuard::new(move |record| async move {
                match record {
                    AdmissionRecord::Failed => state.record_failure(&owner).await,
                    AdmissionRecord::Finished { outcome, docs, errors } => {
                        state.record_outcome(&owner, outcome, docs, errors).await
                    }
                }
            }))
        })
    }

    fn exporter(&self) -> BoxFuture<'_, Result<Exporter>> {
        Box::pin(async move { Ok(self.0.exporter.read().await.clone()) })
    }
}

const PATH: &str = "/mcp";

/// The `/mcp` route: one `EsdiagMcp` shared by every request, so requests
/// share its configuration-write lock and host. Mount [`host_guard`] with
/// `allowed` outside the authentication layer.
pub(crate) fn router(
    state: Arc<ServerState>,
    allowed: &AllowedHosts,
    shutdown: CancellationToken,
) -> Router<Arc<ServerState>> {
    let host: Arc<dyn McpHost> = Arc::new(GuiMcpHost(Arc::clone(&state)));
    let policy = match state.runtime_mode {
        RuntimeMode::User => McpPolicy::Local(host),
        RuntimeMode::Service => McpPolicy::Service(host),
    };
    let mcp = EsdiagMcp::new(policy);
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_cancellation_token(shutdown)
        .with_allowed_hosts(allowed.authorities())
        .with_allowed_origins(allowed.origins());
    let service = StreamableHttpService::new(
        move || Ok(mcp.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new().route_service(PATH, service)
}

#[cfg(test)]
mod tests;
