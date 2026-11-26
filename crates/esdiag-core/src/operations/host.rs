// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::host_result;
use crate::{
    cli_output::{CliOutcome, CommandOutcome, HostResult},
    client::Client,
    data::{
        Application, Auth, HostRole, KnownHost, KnownHostBuilder, KnownHostCliUpdate, PersistableHost, SecretAuth,
        Settings, Uri, keystore_exists, resolve_secret_auth, with_scoped_keystore_password,
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
    let name = &input.name;
    tracing::info!("Adding host {name}");
    let hosts = KnownHost::parse_hosts_yml()?;
    if hosts.contains_key(name) {
        return Err(eyre!("Host '{name}' already exists"));
    }

    let host = input.build_host()?;
    let mut draft = host.prepare_save(name.clone(), hosts)?;
    let mut update = input.update;
    let needs_default = update.secret.is_none();
    let lookup = resolve_add_host_secret_auth(name, &mut update, input.keystore_password)?;
    if needs_default && let Some(secret) = update.secret {
        draft.set_default_secret(secret);
    }
    let host = save_host(input.name, draft.into_persistable()?, lookup, input.keystore_password).await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostAdded { host }))
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
    let hosts = KnownHost::parse_hosts_yml()?;
    let existing = hosts.get(&name).ok_or_else(|| eyre!("Host '{name}' not found"))?;
    let host = existing.merge_cli_update(&update)?;
    let host = host.prepare_save(name.clone(), hosts)?.into_persistable()?;
    let lookup = resolve_host_secret_auth(update.secret.as_deref(), keystore_password)?;
    let host = save_host(name, host, lookup, keystore_password).await?;
    Ok(CommandOutcome::Completed(CliOutcome::HostUpdated { host }))
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

impl HostAddInput {
    fn build_host(&self) -> Result<KnownHost> {
        if !self.url_template
            && let Some(host) = KnownHost::resolve_template_reference(&self.target)?
        {
            return host.merge_cli_update(&self.update);
        }

        let mut builder = if self.url_template {
            KnownHostBuilder::new_template(self.target.clone())
        } else {
            let url = Url::parse(&self.target).map_err(|err| eyre!("Invalid host target URL: {err}"))?;
            let app = self.app.or_else(|| infer_product_from_url(&url)).ok_or_else(|| {
                eyre!(
                    "Target '{}' does not determine the app. Rerun with `--app <app>`.",
                    self.target
                )
            })?;
            KnownHostBuilder::new(url).application(app)
        }
        .accept_invalid_certs(self.update.accept_invalid_certs.unwrap_or(false))
        .auth(
            Auth::from_parts(
                self.update.apikey.clone(),
                self.update.username.clone(),
                self.update.password.clone(),
            )
            .map_err(|err| eyre!("Invalid KnownHost configuration: {err}"))?,
        )
        .secret(self.update.secret.clone());
        if let Some(app) = self.app {
            builder = builder.application(app);
        }
        if let Some(roles) = &self.update.roles {
            builder = builder.roles(roles.clone());
        }
        builder.build()
    }
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

async fn save_host(
    name: String,
    host: PersistableHost,
    lookup: SecretLookup,
    password_source: KeystorePasswordSource,
) -> Result<HostResult> {
    if !host.host().is_template() {
        let unasked_source = lookup.unasked_source(password_source);
        let password = lookup.keystore_password.or_else(|| {
            // On failure, validation reports the missing password itself.
            let source = unasked_source.filter(|_| host.host().requires_keystore_secret())?;
            source().ok()
        });
        let validation = validate_host_connection(&name, Uri::try_from(host.host().clone())?);
        match password {
            Some(password) => with_scoped_keystore_password(password, validation).await?,
            None => validation.await?,
        };
    }
    let is_collect_host = host.host().has_role(HostRole::Collect);
    let result = host_result(name, host.host());
    let hostfile = host.save()?;
    if is_collect_host {
        clear_collection_deferral()?;
    }
    tracing::info!("Host {} successfully saved to {hostfile}", result.name);
    Ok(result)
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
                // Direct callers can supply both; an explicit secret still takes precedence.
                username: Some("unused".to_string()),
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
        assert!(error.to_string().contains("esdiag keystore add <id>"), "{error}");

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

    static PASSWORD_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn counting_source() -> Result<Secret<String>> {
        PASSWORD_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Secret::new("pw".to_string()))
    }

    #[tokio::test]
    async fn host_that_cannot_be_saved_is_refused_before_the_password_is_asked() {
        let _env = TestEnv::new();
        PASSWORD_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
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

        // A same-name secret exists, so `host add` would look it up.
        upsert_secret_auth("new-es", SecretAuth::apikey("new-key"), "pw").expect("save secret");
        let view_role = |secret: Option<&str>| KnownHostCliUpdate {
            roles: Some(vec![HostRole::View]),
            secret: secret.map(str::to_string),
            ..Default::default()
        };
        let role_error = "role 'view' is only valid for Kibana hosts";

        for secret in [None, Some("prod-es")] {
            let error = host_update(HostUpdateInput {
                name: "prod-es".to_string(),
                update: view_role(secret),
                keystore_password: counting_source,
            })
            .await
            .expect_err("update to a role the app does not support");
            assert!(error.to_string().contains(role_error), "{secret:?}: {error}");
        }

        let add = |name: &str, target: String, url_template: bool, update: KnownHostCliUpdate| HostAddInput {
            name: name.to_string(),
            app: Some(Application::Elasticsearch),
            target,
            url_template,
            update,
            keystore_password: counting_source,
        };
        let error = host_add(add("new-es", url.clone(), false, view_role(None)))
            .await
            .expect_err("add with a role the app does not support");
        assert!(error.to_string().contains(role_error), "{error}");

        let error = host_add(add(
            "Bad_Template",
            format!("{url}/deployments/{{id}}/"),
            true,
            KnownHostCliUpdate {
                secret: Some("prod-es".to_string()),
                ..Default::default()
            },
        ))
        .await
        .expect_err("add a template with a name that cannot be a URL scheme");
        assert!(
            error.to_string().contains("must start with a lowercase ASCII letter"),
            "{error}"
        );

        assert_eq!(PASSWORD_CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reserved_names_and_invalid_viewer_links_are_rejected_without_a_request() {
        let _env = setup_env();
        let (url, requests) = serve_counting_cluster().await;
        PASSWORD_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
        let target = Url::parse(&url).expect("valid URL");
        upsert_secret_auth("prod-secret", SecretAuth::apikey("prod-key"), "pw").expect("save secret");
        KnownHostBuilder::new(target.clone())
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .expect("build viewer host")
            .save("viewer")
            .expect("save viewer host");
        KnownHostBuilder::new(target)
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .viewer(Some("viewer".into()))
            .secret(Some("prod-secret".into()))
            .build()
            .expect("build output host")
            .save("prod")
            .expect("save output host");
        let before = KnownHost::parse_hosts_yml().unwrap();

        let error = host_update(HostUpdateInput {
            name: "prod".into(),
            update: KnownHostCliUpdate {
                roles: Some(vec![HostRole::Collect]),
                ..Default::default()
            },
            keystore_password: counting_source,
        })
        .await
        .expect_err("the viewer link requires the send role");
        assert!(
            error.to_string().contains("does not have required role 'send'"),
            "{error}"
        );

        let error = host_add(HostAddInput {
            name: ".prod.es".into(),
            app: Some(Application::Elasticsearch),
            target: url,
            url_template: false,
            update: KnownHostCliUpdate {
                secret: Some("prod-secret".into()),
                ..Default::default()
            },
            keystore_password: counting_source,
        })
        .await
        .expect_err("Elastic CLI context references are reserved");
        assert!(
            error
                .to_string()
                .contains("reserved for Elastic CLI context references"),
            "{error}"
        );

        assert_eq!(PASSWORD_CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(KnownHost::parse_hosts_yml().unwrap(), before);
    }

    #[tokio::test]
    async fn template_host_add_asks_the_password_source_once() {
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

        PASSWORD_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
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
        assert_eq!(PASSWORD_CALLS.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
