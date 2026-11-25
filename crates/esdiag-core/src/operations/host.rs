// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::host_result;
use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    client::Client,
    data::{
        Application, Auth, HostRole, KnownHost, KnownHostBuilder, KnownHostCliUpdate, SecretAuth, Settings, Uri,
        keystore_exists, resolve_secret_auth, with_scoped_keystore_password,
    },
    onboarding::clear_collection_deferral,
    receiver::Receiver,
};
use eyre::{Result, eyre};
use redact::Secret;
use std::str::FromStr;
use url::Url;

/// Supplies the keystore password, called only when a secret must be read.
/// The front end chooses whether this may prompt.
pub type KeystorePasswordSource = fn() -> Result<Secret<String>>;

#[derive(Debug, Clone)]
pub struct HostAddInput {
    pub name: String,
    /// Required when the target URL does not determine the application.
    pub app: Option<Application>,
    /// A URL, a URL template when `url_template` is set, or a template reference.
    pub target: String,
    pub url_template: bool,
    pub update: KnownHostCliUpdate,
    pub keystore_password: KeystorePasswordSource,
}

#[derive(Debug, Clone)]
pub struct HostUpdateInput {
    pub name: String,
    pub update: KnownHostCliUpdate,
    pub keystore_password: KeystorePasswordSource,
}

/// Saves a new host after validating its connection, unless it is a URL template.
pub async fn host_add(input: HostAddInput) -> Result<CommandOutcome> {
    let HostAddInput {
        name,
        app,
        target,
        url_template,
        mut update,
        keystore_password,
    } = input;
    tracing::info!("Adding host {name}");
    if KnownHost::get_known(&name).is_some() {
        return Err(eyre!("Host '{name}' already exists"));
    }
    let mut lookup = resolve_add_host_secret_auth(&name, &mut update, keystore_password)?;
    let unasked_source = lookup.unasked_source(keystore_password);
    let secret_auth = lookup.auth.take();
    let host = if url_template {
        build_host_from_definition(app, &target, true, &update, secret_auth)?
    } else if let Some(host) = KnownHost::resolve_template_reference(&target)? {
        host.merge_cli_update(&update, secret_auth)?
    } else {
        build_host_from_definition(app, &target, false, &update, secret_auth)?
    };
    save_host(
        &name,
        host.clone(),
        "added",
        !host.is_template(),
        lookup.keystore_password,
        unasked_source,
    )
    .await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostAdded {
        host: host_result(name, &host),
    }))
}

pub async fn host_update(input: HostUpdateInput) -> Result<CommandOutcome> {
    let HostUpdateInput {
        name,
        update,
        keystore_password,
    } = input;
    tracing::info!("Updating host {name}");
    if update.is_empty() {
        return Err(eyre!(
            "No host update fields were provided. Use `esdiag host auth {name}` to test the saved host without modifying it."
        ));
    }
    let existing = KnownHost::get_known(&name).ok_or_else(|| eyre!("Host '{name}' not found"))?;
    let mut lookup = resolve_host_secret_auth(update.secret.as_deref(), keystore_password)?;
    let unasked_source = lookup.unasked_source(keystore_password);
    let host = existing.merge_cli_update(&update, lookup.auth.take())?;
    save_host(
        &name,
        host.clone(),
        "updated",
        !host.is_template(),
        lookup.keystore_password,
        unasked_source,
    )
    .await?;
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

/// A host that cannot be saved is refused before its password is asked for or
/// its connection is tested, so its credential is never sent.
///
/// Connection validation reads the host's secret again, so the password is
/// scoped over it: the one already obtained while building the host, otherwise
/// one asked of `unasked_source` when validation will read an inherited or
/// template secret. `unasked_source` is `None` once the source has been asked.
async fn save_host(
    name: &str,
    host: KnownHost,
    action: &str,
    validate_connection: bool,
    resolved_password: Option<Secret<String>>,
    unasked_source: Option<KeystorePasswordSource>,
) -> Result<String> {
    let host = host.into_persistable(name)?;
    let keystore_password = match (resolved_password, unasked_source) {
        (Some(password), _) => Some(password),
        // On failure, validation reports the missing password itself.
        (None, Some(source)) if validate_connection && host.requires_keystore_secret() => source().ok(),
        (None, _) => None,
    };
    let save = validate_and_save_host(name, host, action, validate_connection);
    match keystore_password {
        Some(keystore_password) => with_scoped_keystore_password(keystore_password, save).await,
        None => save.await,
    }
}

async fn validate_and_save_host(
    name: &str,
    host: KnownHost,
    action: &str,
    validate_connection: bool,
) -> Result<String> {
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

/// What a secret lookup found, plus the password it obtained, which is kept
/// even when no secret matched so validation never asks the source twice.
#[derive(Default)]
struct SecretLookup {
    auth: Option<SecretAuth>,
    keystore_password: Option<Secret<String>>,
    source_asked: bool,
}

impl SecretLookup {
    /// The source to ask during validation, unless the lookup already asked it.
    fn unasked_source(&self, source: KeystorePasswordSource) -> Option<KeystorePasswordSource> {
        (!self.source_asked).then_some(source)
    }
}

fn resolve_host_secret_auth(
    secret_id: Option<&str>,
    keystore_password: KeystorePasswordSource,
) -> Result<SecretLookup> {
    let Some(secret_id) = secret_id else {
        return Ok(SecretLookup::default());
    };

    let keystore_password = keystore_password()?;
    let auth = resolve_secret_auth(secret_id, keystore_password.expose_secret())?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    Ok(SecretLookup {
        auth: Some(auth),
        keystore_password: Some(keystore_password),
        source_asked: true,
    })
}

fn resolve_add_host_secret_auth(
    host_name: &str,
    update: &mut KnownHostCliUpdate,
    keystore_password: KeystorePasswordSource,
) -> Result<SecretLookup> {
    if update.secret.as_deref().is_some() {
        return resolve_host_secret_auth(update.secret.as_deref(), keystore_password);
    }
    if update.apikey.is_some() || update.username.is_some() || update.password.is_some() {
        return Ok(SecretLookup::default());
    }
    let lookup = resolve_same_name_host_secret_auth(host_name, keystore_password)?;
    if lookup.auth.is_some() {
        tracing::debug!("Using host name {host_name} as secret_id");
        update.secret = Some(host_name.to_string());
    }
    Ok(lookup)
}

fn resolve_same_name_host_secret_auth(
    host_name: &str,
    keystore_password: KeystorePasswordSource,
) -> Result<SecretLookup> {
    if !keystore_exists()? {
        return Ok(SecretLookup::default());
    }
    // The same-name secret is an optional default; without a password the host is saved without it.
    let Ok(keystore_password) = keystore_password() else {
        return Ok(SecretLookup {
            source_asked: true,
            ..SecretLookup::default()
        });
    };
    Ok(SecretLookup {
        auth: resolve_secret_auth(host_name, keystore_password.expose_secret())?,
        keystore_password: Some(keystore_password),
        source_asked: true,
    })
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
        data::{Credential, get_keystore_password, upsert_secret_auth},
        es_mock::ElasticsearchMock,
    };

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

        let resolved = resolve_host_secret_auth(Some("api-secret"), get_keystore_password).expect("resolve auth");
        assert!(matches!(resolved.auth, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_detects_basic() {
        let _env = setup_env();
        upsert_secret_auth("basic-secret", SecretAuth::basic("elastic", "secret-password"), "pw")
            .expect("save basic secret");

        let resolved = resolve_host_secret_auth(Some("basic-secret"), get_keystore_password).expect("resolve auth");
        assert!(matches!(resolved.auth, Some(SecretAuth::Basic { .. })));
    }

    #[test]
    fn host_secret_auth_resolution_reads_named_secret() {
        let _env = setup_env();
        upsert_secret_auth("host-fallback", SecretAuth::apikey("secret-key"), "pw").expect("save fallback secret");

        let resolved = resolve_host_secret_auth(Some("host-fallback"), get_keystore_password).expect("resolve auth");
        assert!(matches!(resolved.auth, Some(SecretAuth::ApiKey { .. })));
    }

    #[test]
    fn add_host_uses_same_name_secret_without_secret_flag() {
        let _tmp = setup_env();
        upsert_secret_auth("prod-es", SecretAuth::apikey("secret-key"), "pw").expect("save secret");

        let mut update = KnownHostCliUpdate::default();
        let resolved =
            resolve_add_host_secret_auth("prod-es", &mut update, get_keystore_password).expect("resolve auth");

        assert!(matches!(resolved.auth, Some(SecretAuth::ApiKey { .. })));
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
        let resolved =
            resolve_add_host_secret_auth("prod-es", &mut update, get_keystore_password).expect("resolve auth");

        assert!(resolved.auth.is_none());
        assert!(update.secret.as_deref().is_none());
    }

    #[test]
    fn add_host_without_keystore_has_no_secret() {
        let _tmp = setup_env();

        let mut update = KnownHostCliUpdate::default();
        let resolved =
            resolve_add_host_secret_auth("prod-es", &mut update, get_keystore_password).expect("resolve auth");

        assert!(resolved.auth.is_none());
        assert!(update.secret.as_deref().is_none());
    }

    #[tokio::test]
    async fn host_validation_uses_the_supplied_keystore_password() {
        let _env = TestEnv::new();
        upsert_secret_auth("prod-es", SecretAuth::apikey("prod-key"), "pw").expect("save secret");
        upsert_secret_auth("rotated", SecretAuth::apikey("rotated-key"), "pw").expect("save secret");
        let mock = ElasticsearchMock::new().require_authorization("ApiKey prod-key");
        let url = mock.serve().await;
        let source: KeystorePasswordSource = || Ok(Secret::new("pw".to_string()));
        let add = |name: &str, secret: Option<&str>| HostAddInput {
            name: name.to_string(),
            app: Some(Application::Elasticsearch),
            target: url.clone(),
            url_template: false,
            update: KnownHostCliUpdate {
                secret: secret.map(str::to_string),
                ..Default::default()
            },
            keystore_password: source,
        };

        host_add(add("prod-es", None))
            .await
            .expect("add with the same-name secret");
        host_add(add("explicit", Some("prod-es")))
            .await
            .expect("add with an explicit secret");

        mock.set_required_authorization("ApiKey rotated-key");
        host_update(HostUpdateInput {
            name: "explicit".to_string(),
            update: KnownHostCliUpdate {
                secret: Some("rotated".to_string()),
                ..Default::default()
            },
            keystore_password: source,
        })
        .await
        .expect("update to another secret");

        host_update(HostUpdateInput {
            name: "explicit".to_string(),
            update: KnownHostCliUpdate {
                accept_invalid_certs: Some(true),
                ..Default::default()
            },
            keystore_password: source,
        })
        .await
        .expect("update that keeps the saved secret");
    }

    /// Serves a cluster that rejects every request and counts what it received.
    async fn serve_counting_cluster() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = requests.clone();
        let app = axum::Router::new().fallback(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { axum::http::StatusCode::UNAUTHORIZED }
        });
        (ElasticsearchMock::serve_router(app).await, requests)
    }

    #[tokio::test]
    async fn host_that_cannot_be_saved_is_refused_without_a_request() {
        let _env = setup_env();
        let (url, requests) = serve_counting_cluster().await;
        KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse(&url).expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        )
        .save("prod-es")
        .expect("save host");
        let saved = KnownHost::get_known(&"prod-es".to_string()).expect("saved host");
        let inline_key = || KnownHostCliUpdate {
            apikey: Some(Secret::new("inline-key".to_string())),
            ..Default::default()
        };
        let secret_error = "requires a secret reference before it can be saved";

        let error = host_add(HostAddInput {
            name: "inline-es".to_string(),
            app: Some(Application::Elasticsearch),
            target: url.clone(),
            url_template: false,
            update: inline_key(),
            keystore_password: get_keystore_password,
        })
        .await
        .expect_err("add with an inline key");
        assert!(error.to_string().contains(secret_error), "{error}");

        let updates = [
            (inline_key(), secret_error),
            (
                KnownHostCliUpdate {
                    username: Some("elastic".to_string()),
                    password: Some(Secret::new("inline-password".to_string())),
                    ..Default::default()
                },
                secret_error,
            ),
            (
                KnownHostCliUpdate {
                    roles: Some(vec![HostRole::View]),
                    ..Default::default()
                },
                "role 'view' is only valid for Kibana hosts",
            ),
        ];
        for (update, expected) in updates {
            let error = host_update(HostUpdateInput {
                name: "prod-es".to_string(),
                update,
                keystore_password: get_keystore_password,
            })
            .await
            .expect_err("update to a host that cannot be saved");
            assert!(error.to_string().contains(expected), "{error}");
        }

        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(KnownHost::get_known(&"inline-es".to_string()).is_none());
        assert_eq!(KnownHost::get_known(&"prod-es".to_string()), Some(saved));
    }

    #[tokio::test]
    async fn host_that_cannot_be_saved_is_refused_before_the_password_is_asked() {
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn counting_source() -> Result<Secret<String>> {
            CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Secret::new("pw".to_string()))
        }

        let _env = TestEnv::new();
        let (url, requests) = serve_counting_cluster().await;
        KnownHost::new(
            Application::Elasticsearch,
            Url::parse(&url).expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
            Credential::Keystore("prod-es".to_string()),
        )
        .save("prod-es")
        .expect("save host");

        let error = host_update(HostUpdateInput {
            name: "prod-es".to_string(),
            update: KnownHostCliUpdate {
                roles: Some(vec![HostRole::View]),
                ..Default::default()
            },
            keystore_password: counting_source,
        })
        .await
        .expect_err("update to a role the app does not support");
        assert!(
            error.to_string().contains("role 'view' is only valid for Kibana hosts"),
            "{error}"
        );
        assert_eq!(CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn template_host_add_asks_the_password_source_once() {
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn counting_source() -> Result<Secret<String>> {
            CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Secret::new("pw".to_string()))
        }

        let _env = TestEnv::new();
        upsert_secret_auth("tmpl-secret", SecretAuth::apikey("tmpl-key"), "pw").expect("save secret");
        let mock = ElasticsearchMock::new()
            .path_prefix("/deployments/dep1")
            .require_authorization("ApiKey tmpl-key");
        let url = mock.serve().await;
        host_add(HostAddInput {
            name: "tmpl".to_string(),
            app: Some(Application::Elasticsearch),
            target: format!("{url}/deployments/{{id}}/"),
            url_template: true,
            update: KnownHostCliUpdate {
                secret: Some("tmpl-secret".to_string()),
                ..Default::default()
            },
            keystore_password: counting_source,
        })
        .await
        .expect("save template host");

        CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        host_add(HostAddInput {
            name: "dep-host".to_string(),
            app: None,
            target: "tmpl://dep1".to_string(),
            url_template: false,
            update: KnownHostCliUpdate::default(),
            keystore_password: counting_source,
        })
        .await
        .expect("add a concrete host from the template");
        assert_eq!(CALLS.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
