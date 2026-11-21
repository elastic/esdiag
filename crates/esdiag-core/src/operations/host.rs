// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::host_result;
use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    client::Client,
    data::{
        Application, Auth, HostRole, KnownHost, KnownHostBuilder, KnownHostCliUpdate, SecretAuth, Settings, Uri,
        get_password_for_secret_commands, keystore_exists, resolve_secret_auth,
    },
    onboarding::clear_collection_deferral,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use std::str::FromStr;
use url::Url;

#[derive(Debug, Clone)]
pub struct HostAddInput {
    pub name: String,
    /// Required when the target URL does not determine the application.
    pub app: Option<Application>,
    /// A URL, a URL template when `url_template` is set, or a template reference.
    pub target: String,
    pub url_template: bool,
    pub update: KnownHostCliUpdate,
}

#[derive(Debug, Clone)]
pub struct HostUpdateInput {
    pub name: String,
    pub update: KnownHostCliUpdate,
}

/// Saves a new host after validating its connection, unless it is a URL template.
pub async fn host_add(input: HostAddInput) -> Result<CommandOutcome> {
    let HostAddInput {
        name,
        app,
        target,
        url_template,
        mut update,
    } = input;
    tracing::info!("Adding host {name}");
    if KnownHost::get_known(&name).is_some() {
        return Err(eyre!("Host '{name}' already exists"));
    }
    let secret_auth = resolve_add_host_secret_auth(&name, &mut update)?;
    let host = if url_template {
        build_host_from_definition(app, &target, true, &update, secret_auth)?
    } else if let Some(host) = KnownHost::resolve_template_reference(&target)? {
        host.merge_cli_update(&update, secret_auth)?
    } else {
        build_host_from_definition(app, &target, false, &update, secret_auth)?
    };
    save_host(&name, host.clone(), "added", !host.is_template()).await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostAdded {
        host: host_result(name, &host),
    }))
}

pub async fn host_update(input: HostUpdateInput) -> Result<CommandOutcome> {
    let HostUpdateInput { name, update } = input;
    tracing::info!("Updating host {name}");
    if update.is_empty() {
        return Err(eyre!(
            "No host update fields were provided. Use `esdiag host auth {name}` to test the saved host without modifying it."
        ));
    }
    let existing = KnownHost::get_known(&name).ok_or_else(|| eyre!("Host '{name}' not found"))?;
    let secret_auth = if update.secret.as_deref().is_some() {
        resolve_host_secret_auth(update.secret.as_deref())?
    } else {
        None
    };
    let host = existing.merge_cli_update(&update, secret_auth)?;
    save_host(&name, host.clone(), "updated", !host.is_template()).await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostUpdated {
        host: host_result(name, &host),
    }))
}

pub fn host_remove(name: String) -> Result<CommandOutcome> {
    tracing::info!("Removing host {name}");
    let is_collect_host = KnownHost::get_known(&name).is_some_and(|host| host.has_role(HostRole::Collect));
    let path = KnownHost::remove_saved(&name)?;
    if is_collect_host {
        clear_collection_deferral()?;
    }
    if let Err(err) = cleanup_settings_after_host_delete(&name) {
        tracing::warn!("Host '{name}' was removed, but failed to update settings: {err}");
    }
    tracing::info!("Host {name} successfully deleted from {path}");
    Ok(CommandOutcome::Completed(CliOutcome::HostRemoved { name, path }))
}

pub fn host_list() -> Result<CommandOutcome> {
    let hosts = KnownHost::parse_hosts_yml()?
        .into_iter()
        .map(|(name, host)| host_result(name, &host))
        .collect();
    Ok(CommandOutcome::Completed(CliOutcome::HostsListed { hosts }))
}

/// Tests the connection of a saved host or a resolved template reference.
pub async fn host_auth(target: String) -> Result<CommandOutcome> {
    tracing::info!("Testing saved host {target}");
    if let Some(host) = KnownHost::resolve_template_reference(&target)? {
        validate_host_connection(&target, Uri::try_from(host)?).await?;
        return Ok(CommandOutcome::Completed(CliOutcome::HostAuthenticated {
            name: target,
            message: None,
        }));
    }
    let host = KnownHost::get_known(&target).ok_or_else(|| eyre!("Host '{target}' not found"))?;
    if host.is_template() {
        return Ok(CommandOutcome::Completed(CliOutcome::HostAuthenticated {
            name: target.clone(),
            message: Some(KnownHost::template_guidance(&target)),
        }));
    }
    let uri = Uri::try_from(host)?;
    validate_host_connection(&target, uri).await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostAuthenticated {
        name: target,
        message: None,
    }))
}

fn infer_product_from_url(url: &Url) -> Option<Application> {
    match url.port_or_known_default() {
        Some(9200) => Some(Application::Elasticsearch),
        Some(5601) => Some(Application::Kibana),
        Some(9600) => Some(Application::Logstash),
        _ => {
            let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
            if matches!(segments.next(), Some("api"))
                && matches!(segments.next(), Some("v1"))
                && matches!(segments.next(), Some("deployments"))
            {
                let _deployment_id = segments.next()?;
                return Application::from_str(segments.next()?).ok();
            }
            None
        }
    }
}

fn build_host_from_definition(
    app: Option<Application>,
    target: &str,
    use_url_template: bool,
    update: &KnownHostCliUpdate,
    secret_auth: Option<SecretAuth>,
) -> Result<KnownHost> {
    let mut builder = if use_url_template {
        KnownHostBuilder::new_template(target.to_string())
    } else {
        let url = Url::parse(target).map_err(|err| eyre!("Invalid host target URL: {err}"))?;
        let inferred_app = infer_product_from_url(&url);
        let app = app
            .or(inferred_app)
            .ok_or_else(|| eyre!("Target '{target}' does not determine the app. Rerun with `--app <app>`."))?;
        KnownHostBuilder::new(url).application(app)
    }
    .accept_invalid_certs(update.accept_invalid_certs.unwrap_or(false))
    .auth(
        Auth::from_parts(update.apikey.clone(), update.username.clone(), update.password.clone())
            .map_err(|err| eyre!("Invalid KnownHost configuration: {err}"))?,
    )
    .secret(update.secret.clone());
    if let Some(app) = app {
        builder = builder.application(app);
    }
    if let Some(roles) = update.roles.clone() {
        builder = builder.roles(roles);
    }
    match secret_auth {
        Some(secret_auth) => builder.build_with_secret_auth(secret_auth),
        None => builder.build(),
    }
}

async fn save_host(name: &str, host: KnownHost, action: &str, validate_connection: bool) -> Result<String> {
    let is_collect_host = host.has_role(HostRole::Collect);
    if validate_connection {
        let uri = Uri::try_from(host.clone())?;
        let validation_summary = validate_host_connection(name, uri).await?;
        let hostfile = host.save(name)?;
        if is_collect_host {
            clear_collection_deferral()?;
        }
        tracing::info!("Host {name} successfully saved to {hostfile}");
        return Ok(format!("{validation_summary}\nHost '{name}' {action} in {hostfile}"));
    }
    let hostfile = host.save(name)?;
    if is_collect_host {
        clear_collection_deferral()?;
    }
    tracing::info!("Host {name} successfully saved to {hostfile}");
    Ok(format!("Host '{name}' {action} in {hostfile}"))
}

fn cleanup_settings_after_host_delete(name: &str) -> Result<()> {
    let mut settings = Settings::load()?;
    if settings.active_target.as_deref() != Some(name) {
        return Ok(());
    }

    let hosts = KnownHost::parse_hosts_yml()?;
    settings.active_target = hosts.keys().next().cloned();
    settings.save()
}

fn resolve_host_secret_auth(secret_id: Option<&str>) -> Result<Option<SecretAuth>> {
    let Some(secret_id) = secret_id else {
        return Ok(None);
    };

    let keystore_password = get_password_for_secret_commands()?;
    let secret_auth = resolve_secret_auth(secret_id, keystore_password.expose_secret())?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    Ok(Some(secret_auth))
}

fn resolve_add_host_secret_auth(host_name: &str, update: &mut KnownHostCliUpdate) -> Result<Option<SecretAuth>> {
    if update.secret.as_deref().is_some() {
        return resolve_host_secret_auth(update.secret.as_deref());
    }
    if update.apikey.is_some() || update.username.is_some() || update.password.is_some() {
        return Ok(None);
    }
    let secret_auth = resolve_same_name_host_secret_auth(host_name)?;
    if secret_auth.is_some() {
        tracing::debug!("Using host name {host_name} as secret_id");
        update.secret = Some(host_name.to_string());
    }
    Ok(secret_auth)
}

fn resolve_same_name_host_secret_auth(host_name: &str) -> Result<Option<SecretAuth>> {
    if !keystore_exists()? {
        return Ok(None);
    }
    let Ok(keystore_password) = get_password_for_secret_commands() else {
        return Ok(None);
    };
    resolve_secret_auth(host_name, keystore_password.expose_secret())
}

fn host_connection_uses_receiver(uri: &Uri) -> bool {
    matches!(uri, Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_))
}

async fn validate_host_connection(name: &str, uri: Uri) -> Result<String> {
    if host_connection_uses_receiver(&uri) {
        let receiver = Receiver::try_from(uri)?;
        if receiver.is_connected().await {
            let summary = format!("Host {name}: connected to Elastic Cloud Admin proxy");
            tracing::info!("{summary}");
            return Ok(summary);
        }

        tracing::error!("Host connection: FAILED ❌ Elastic Cloud Admin proxy connection failed");
        tracing::warn!("Check your URL, certificates, and secret credentials!");
        return Err(eyre!("Host connection failed"));
    }

    match Client::try_from(uri)?.test_connection().await {
        Ok(message) => {
            let summary = format!("Host {name}: {message}");
            tracing::info!("{summary}");
            Ok(summary)
        }
        Err(message) => {
            tracing::error!("Host connection: FAILED ❌ {}", &message);
            tracing::warn!("Check your URL and certificates!");
            Err(eyre!("Host connection failed"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TestEnv,
        data::{Credential, upsert_secret_auth},
    };
    use redact::Secret;

    fn setup_env() -> TestEnv {
        let mut env = TestEnv::new();
        env.set("ESDIAG_KEYSTORE_PASSWORD", "pw");
        env
    }

    #[test]
    fn elastic_cloud_admin_hosts_validate_via_receiver() {
        let uri = Uri::ElasticCloudAdmin(KnownHost::new(
            Application::Elasticsearch,
            Url::parse("https://admin.found.no/api/v1/deployments/test/elasticsearch/main-elasticsearch/proxy/")
                .expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
            Credential::Keystore("ada-admin".to_string()),
        ));

        assert!(host_connection_uses_receiver(&uri));
    }

    #[test]
    fn standard_known_hosts_validate_via_client() {
        let uri = Uri::KnownHost(KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        ));

        assert!(!host_connection_uses_receiver(&uri));
    }

    #[test]
    fn host_secret_auth_resolution_detects_apikey() {
        let _env = setup_env();
        upsert_secret_auth("api-secret", SecretAuth::apikey("secret-key"), "pw").expect("save api secret");

        let resolved = resolve_host_secret_auth(Some("api-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_detects_basic() {
        let _env = setup_env();
        upsert_secret_auth("basic-secret", SecretAuth::basic("elastic", "secret-password"), "pw")
            .expect("save basic secret");

        let resolved = resolve_host_secret_auth(Some("basic-secret")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::Basic { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_reads_named_secret() {
        let _env = setup_env();
        upsert_secret_auth("host-fallback", SecretAuth::apikey("secret-key"), "pw").expect("save fallback secret");

        let resolved = resolve_host_secret_auth(Some("host-fallback")).expect("resolve auth");
        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn add_host_uses_same_name_secret_without_secret_flag() {
        let _tmp = setup_env();
        upsert_secret_auth("prod-es", SecretAuth::apikey("secret-key"), "pw").expect("save secret");

        let mut update = KnownHostCliUpdate::default();
        let resolved = resolve_add_host_secret_auth("prod-es", &mut update).expect("resolve auth");

        assert!(matches!(resolved, Some(SecretAuth::ApiKey { .. })));
        assert_eq!(update.secret.as_deref(), Some("prod-es"));
    }

    #[test]
    fn add_host_skips_same_name_secret_with_explicit_credentials() {
        let _tmp = setup_env();
        upsert_secret_auth("prod-es", SecretAuth::apikey("secret-key"), "pw").expect("save secret");

        let mut update = KnownHostCliUpdate {
            apikey: Some(Secret::new("inline-key".to_string())),
            ..Default::default()
        };
        let resolved = resolve_add_host_secret_auth("prod-es", &mut update).expect("resolve auth");

        assert!(resolved.is_none());
        assert!(update.secret.as_deref().is_none());
    }

    #[test]
    fn add_host_without_keystore_has_no_secret() {
        let _tmp = setup_env();

        let mut update = KnownHostCliUpdate::default();
        let resolved = resolve_add_host_secret_auth("prod-es", &mut update).expect("resolve auth");

        assert!(resolved.is_none());
        assert!(update.secret.as_deref().is_none());
    }
}
