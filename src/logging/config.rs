// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use esdiag_core::{
    client::{ElasticsearchBuilder, ElasticsearchClient, elasticsearch_client_from_output_host},
    data::{Application, ApplicationConfig, Auth, HostRole, KnownHost},
};
use eyre::{Result, eyre};
use redact::Secret;
use std::env;
use url::Url;

/// Default Elasticsearch data stream for ESDiag application logs.
pub const DEFAULT_LOGS_DATA_STREAM: &str = "logs-esdiag-default";

/// Resolved remote Elasticsearch logs target configuration.
pub struct LogsTargetConfig {
    pub client: ElasticsearchClient,
    pub data_stream: String,
    pub source: LogsTargetSource,
    pub insecure: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogsTargetSource {
    EnvironmentUrl,
    EnvironmentHost,
    PersistedHost,
}

impl LogsTargetConfig {
    /// Resolves the optional remote Elasticsearch logs target.
    ///
    /// Precedence:
    /// 1. `ESDIAG_LOGS_URL` (+ `ESDIAG_LOGS_APIKEY` or `ESDIAG_LOGS_USERNAME`/`ESDIAG_LOGS_PASSWORD` + `ESDIAG_LOGS_INSECURE`).
    /// 2. `ESDIAG_LOGS_HOST` referencing a saved Elasticsearch host with role `send`.
    /// 3. `logs.host` in `~/.esdiag/esdiag.yml` referencing a saved Elasticsearch host with role `send`.
    /// 4. `Ok(None)` when unconfigured. Never falls back to `ESDIAG_OUTPUT_URL` or `output.default`.
    pub fn resolve() -> Result<Option<Self>> {
        let data_stream = resolve_data_stream();

        if let Some(raw_url) = non_empty_env("ESDIAG_LOGS_URL") {
            let url = Url::parse(&raw_url).map_err(|err| eyre!("Invalid ESDIAG_LOGS_URL: {err}"))?;
            let auth = resolve_env_auth()?;
            let insecure = parse_env_bool("ESDIAG_LOGS_INSECURE");
            let client = ElasticsearchBuilder::new(url).auth(auth).insecure(insecure).build()?;
            return Ok(Some(Self {
                client,
                data_stream,
                source: LogsTargetSource::EnvironmentUrl,
                insecure,
            }));
        }

        if let Some(host_name) = non_empty_env("ESDIAG_LOGS_HOST") {
            let (client, insecure) = resolve_saved_logs_host(&host_name)?;
            return Ok(Some(Self {
                client,
                data_stream,
                source: LogsTargetSource::EnvironmentHost,
                insecure,
            }));
        }

        let config = ApplicationConfig::load()?;
        if let Some(host_name) = config.logs.host.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let (client, insecure) = resolve_saved_logs_host(host_name)?;
            return Ok(Some(Self {
                client,
                data_stream,
                source: LogsTargetSource::PersistedHost,
                insecure,
            }));
        }

        Ok(None)
    }
}

pub fn resolve_data_stream() -> String {
    non_empty_env("ESDIAG_LOGS_DATA_STREAM").unwrap_or_else(|| DEFAULT_LOGS_DATA_STREAM.to_string())
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_env_bool(name: &str) -> bool {
    non_empty_env(name).is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
}

fn resolve_env_auth() -> Result<Auth> {
    let apikey = non_empty_env("ESDIAG_LOGS_APIKEY");
    let username = non_empty_env("ESDIAG_LOGS_USERNAME");
    let password = non_empty_env("ESDIAG_LOGS_PASSWORD");

    match (apikey, username, password) {
        (Some(key), None, None) => Ok(Auth::Apikey(Secret::new(key))),
        (None, Some(user), Some(pass)) => Ok(Auth::Basic(user, Secret::new(pass))),
        (None, None, None) => Ok(Auth::None),
        (Some(_), _, _) => Err(eyre!(
            "ESDIAG_LOGS_APIKEY cannot be combined with ESDIAG_LOGS_USERNAME or ESDIAG_LOGS_PASSWORD"
        )),
        (None, Some(_), None) | (None, None, Some(_)) => Err(eyre!(
            "ESDIAG_LOGS_USERNAME and ESDIAG_LOGS_PASSWORD must be configured together"
        )),
    }
}

fn resolve_saved_logs_host(name: &str) -> Result<(ElasticsearchClient, bool)> {
    let host = KnownHost::get_known(&name.to_string())
        .ok_or_else(|| eyre!("Configured logs host '{name}' was not found in hosts.yml"))?;
    if host.app() != Some(Application::Elasticsearch) || !host.has_role(HostRole::Send) {
        return Err(eyre!(
            "Configured logs host '{name}' must be an Elasticsearch host with role 'send'"
        ));
    }
    let insecure = host.accept_invalid_certs();
    let client = elasticsearch_client_from_output_host(host)?;
    Ok((client, insecure))
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_LOGS_DATA_STREAM, LogsTargetConfig, LogsTargetSource, resolve_data_stream};
    use esdiag_core::data::{Application, ApplicationConfig, HostRole, KnownHost, KnownHostBuilder, LogsConfig};
    use std::collections::BTreeMap;
    use url::Url;

    fn clear_logs_env(env: &mut esdiag_core::TestEnv) {
        for name in [
            "ESDIAG_LOGS_URL",
            "ESDIAG_LOGS_APIKEY",
            "ESDIAG_LOGS_USERNAME",
            "ESDIAG_LOGS_PASSWORD",
            "ESDIAG_LOGS_INSECURE",
            "ESDIAG_LOGS_HOST",
            "ESDIAG_LOGS_DATA_STREAM",
            "ESDIAG_OUTPUT_URL",
            "ESDIAG_OUTPUT_APIKEY",
        ] {
            env.remove(name);
        }
    }

    #[test]
    fn disabled_when_unconfigured_even_if_output_url_is_set() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");

        let resolved = LogsTargetConfig::resolve().expect("resolve");
        assert!(resolved.is_none());
    }

    #[test]
    fn environment_url_and_apikey_take_precedence_and_respect_insecure_and_data_stream() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);
        env.set("ESDIAG_LOGS_URL", "https://logs.example:9200");
        env.set("ESDIAG_LOGS_APIKEY", "test-key");
        env.set("ESDIAG_LOGS_INSECURE", "true");
        env.set("ESDIAG_LOGS_DATA_STREAM", "logs-esdiag-prod");
        env.set("ESDIAG_LOGS_HOST", "nonexistent-host");

        let resolved = LogsTargetConfig::resolve()
            .expect("resolve")
            .expect("logs target configured");
        assert_eq!(resolved.source, LogsTargetSource::EnvironmentUrl);
        assert_eq!(resolved.data_stream, "logs-esdiag-prod");
        assert!(resolved.insecure);
    }

    #[test]
    fn environment_url_defaults_to_strict_tls_and_default_data_stream() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);
        env.set("ESDIAG_LOGS_URL", "https://logs.example:9200");

        let resolved = LogsTargetConfig::resolve()
            .expect("resolve")
            .expect("logs target configured");
        assert_eq!(resolved.source, LogsTargetSource::EnvironmentUrl);
        assert_eq!(resolved.data_stream, DEFAULT_LOGS_DATA_STREAM);
        assert!(!resolved.insecure);
    }

    #[test]
    fn environment_auth_conflicts_return_errors() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);
        env.set("ESDIAG_LOGS_URL", "https://logs.example:9200");
        env.set("ESDIAG_LOGS_APIKEY", "key");
        env.set("ESDIAG_LOGS_USERNAME", "user");

        let err = LogsTargetConfig::resolve().err().expect("conflict must fail");
        assert!(err.to_string().contains("cannot be combined"));

        env.remove("ESDIAG_LOGS_APIKEY");
        let err = LogsTargetConfig::resolve().err().expect("missing password must fail");
        assert!(err.to_string().contains("configured together"));
    }

    #[test]
    fn environment_host_and_persisted_host_resolve_saved_send_elasticsearch_host() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);

        let logs_es = KnownHostBuilder::new(Url::parse("https://saved-logs.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .build()
            .expect("host");
        KnownHost::write_hosts_yml(&BTreeMap::from([("ops-es".to_string(), logs_es)])).expect("write hosts");

        env.set("ESDIAG_LOGS_HOST", "ops-es");
        let resolved = LogsTargetConfig::resolve().expect("resolve").expect("env host");
        assert_eq!(resolved.source, LogsTargetSource::EnvironmentHost);
        assert_eq!(resolved.data_stream, DEFAULT_LOGS_DATA_STREAM);

        env.remove("ESDIAG_LOGS_HOST");
        let config = ApplicationConfig {
            version: 1,
            logs: LogsConfig {
                host: Some("ops-es".to_string()),
                ..LogsConfig::default()
            },
            ..ApplicationConfig::new()
        };
        config.save().expect("save config");

        let resolved = LogsTargetConfig::resolve().expect("resolve").expect("persisted host");
        assert_eq!(resolved.source, LogsTargetSource::PersistedHost);
    }

    #[test]
    fn saved_host_without_send_role_or_non_es_is_rejected() {
        let mut env = esdiag_core::TestEnv::new();
        clear_logs_env(&mut env);

        let collect_es = KnownHostBuilder::new(Url::parse("https://collect.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .expect("collect host");
        let kibana = KnownHostBuilder::new(Url::parse("https://kb.example:5601").expect("url"))
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .expect("kibana host");
        KnownHost::write_hosts_yml(&BTreeMap::from([
            ("collect-es".to_string(), collect_es),
            ("viewer-kb".to_string(), kibana),
        ]))
        .expect("write hosts");

        for host in ["collect-es", "viewer-kb", "missing"] {
            env.set("ESDIAG_LOGS_HOST", host);
            assert!(LogsTargetConfig::resolve().is_err(), "{host} must be rejected");
        }
        assert_eq!(resolve_data_stream(), DEFAULT_LOGS_DATA_STREAM);
    }
}
