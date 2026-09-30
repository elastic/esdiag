// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Flow-neutral operations used by terminal and future GUI onboarding.

use crate::data::{
    Application, ApplicationConfig, HostRole, Job, JobOutput, KnownHost, KnownHostBuilder, OnboardingWorkflow,
    OutputDeployment, SavedJobs, SecretAuth, keystore_exists, runtime_output_is_declared, save_saved_jobs,
    upsert_secret_auth,
};
use crate::job::model::Input;
use eyre::{Result, eyre};
use url::Url;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OnboardingReadiness {
    pub user_configured: bool,
    pub keystore_ready: bool,
    pub workflow: Option<OnboardingWorkflow>,
    pub output_configured: bool,
    pub output_from_environment: bool,
    pub output_requires_keystore: bool,
    pub collect_host_configured: bool,
    pub default_job_configured: bool,
    pub collection_deferred: bool,
}

impl OnboardingReadiness {
    pub fn is_complete(&self) -> bool {
        self.user_configured
            && match self.workflow {
                Some(OnboardingWorkflow::CollectOnly) => self.collect_host_configured && self.default_job_configured,
                Some(OnboardingWorkflow::ProcessExisting) => {
                    self.output_configured && (self.output_from_environment || self.keystore_ready)
                }
                Some(OnboardingWorkflow::CollectAndProcess) => {
                    self.keystore_ready
                        && self.output_configured
                        && self.collect_host_configured
                        && self.default_job_configured
                }
                None => false,
            }
    }

    pub fn can_enter_application(&self) -> bool {
        self.is_complete()
            || (self.collection_deferred
                && self.user_configured
                && !self.collect_host_configured
                && match self.workflow {
                    Some(OnboardingWorkflow::CollectOnly) => true,
                    Some(OnboardingWorkflow::CollectAndProcess) => {
                        self.output_configured
                            && (self.output_from_environment || !self.output_requires_keystore || self.keystore_ready)
                    }
                    Some(OnboardingWorkflow::ProcessExisting) | None => false,
                })
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeploymentInput {
    pub output_name: String,
    pub output_url: Url,
    pub viewer_name: String,
    pub viewer_url: Url,
    pub secret_id: String,
    pub auth: SecretAuth,
}

#[derive(Clone, Debug)]
pub struct CollectHostInput {
    pub name: String,
    pub app: Application,
    pub url: Url,
    pub secret_id: Option<String>,
    pub auth: Option<SecretAuth>,
}

/// Build a transient validation target without resolving an unsaved secret.
/// Persisted hosts are built separately and contain only a keystore reference.
pub fn validation_host(url: Url, app: Application, auth: SecretAuth) -> Result<KnownHost> {
    let mut host = KnownHostBuilder::new(url).application(app).build()?;
    match auth {
        SecretAuth::ApiKey { apikey } => host.legacy_apikey = Some(apikey),
        SecretAuth::Basic { username, password } => {
            host.legacy_username = Some(username);
            host.legacy_password = Some(password);
        }
    }
    Ok(host)
}

pub fn output_validation_host(url: Url, app: Application, auth: SecretAuth) -> Result<KnownHost> {
    validation_host(url, app, auth)
}

pub fn inspect() -> Result<OnboardingReadiness> {
    let config = ApplicationConfig::load()?;
    let hosts = KnownHost::parse_hosts_yml()?;
    let jobs = crate::data::load_saved_jobs()?;

    let output_declared_by_environment = runtime_output_is_declared();
    let output_from_environment = output_declared_by_environment && OutputDeployment::resolve(None, true).is_ok();
    let output_configured = if output_declared_by_environment {
        output_from_environment
    } else {
        config
            .output
            .default
            .as_ref()
            .is_some_and(|name| hosts.get(name).is_some_and(valid_output_host))
    };
    let output_requires_keystore = !output_from_environment
        && !output_declared_by_environment
        && config
            .output
            .default
            .as_ref()
            .and_then(|name| hosts.get(name))
            .is_some_and(KnownHost::requires_keystore_secret);
    let collect_host_configured = hosts.values().any(|host| host.has_role(HostRole::Collect));
    let default_job_configured = config
        .job
        .default
        .as_ref()
        .and_then(|name| jobs.get(name))
        .is_some_and(|job| workflow_job_is_valid(job, &hosts, config.workflow, config.output.default.as_deref()));

    Ok(OnboardingReadiness {
        user_configured: config.user.as_deref().is_some_and(|user| !user.trim().is_empty()),
        keystore_ready: keystore_exists()?,
        workflow: config.workflow,
        output_configured,
        output_from_environment,
        output_requires_keystore,
        collect_host_configured,
        default_job_configured,
        collection_deferred: config.collection_deferred,
    })
}

fn workflow_job_is_valid(
    job: &Job,
    hosts: &std::collections::BTreeMap<String, KnownHost>,
    workflow: Option<OnboardingWorkflow>,
    output: Option<&str>,
) -> bool {
    let Input::Collect { host, .. } = job.input() else {
        return false;
    };
    if !hosts.get(host).is_some_and(|host| host.has_role(HostRole::Collect)) {
        return false;
    }
    match workflow {
        Some(OnboardingWorkflow::CollectOnly) => job.process().is_none(),
        Some(OnboardingWorkflow::CollectAndProcess) => {
            matches!(
                job.process().map(|process| &process.export),
                Some(JobOutput::Environment) if environment_output_is_configured()
            ) || matches!(
                job.process().map(|process| &process.export),
                Some(JobOutput::KnownHost { name }) if Some(name.as_str()) == output
            )
        }
        Some(OnboardingWorkflow::ProcessExisting) | None => false,
    }
}

fn environment_output_is_configured() -> bool {
    runtime_output_is_declared() && OutputDeployment::resolve(None, true).is_ok()
}

/// Whether the saved job `name` is a valid default job for the configured
/// workflow and output deployment.
pub fn saved_job_fits_workflow(name: &str) -> Result<bool> {
    let config = ApplicationConfig::load()?;
    let hosts = KnownHost::parse_hosts_yml()?;
    Ok(crate::data::load_saved_jobs()?
        .get(name)
        .is_some_and(|job| workflow_job_is_valid(job, &hosts, config.workflow, config.output.default.as_deref())))
}

pub fn save_user(user: String) -> Result<ApplicationConfig> {
    let user = user.trim();
    if user.is_empty() {
        return Err(eyre!("A default diagnostic user is required"));
    }
    let mut config = ApplicationConfig::load()?;
    config.user = Some(user.to_string());
    config.save()?;
    Ok(config)
}

pub fn save_workflow(workflow: OnboardingWorkflow) -> Result<ApplicationConfig> {
    let mut config = ApplicationConfig::load()?;
    if config.workflow != Some(workflow) {
        config.collection_deferred = false;
    }
    config.workflow = Some(workflow);
    config.save()?;
    Ok(config)
}

pub fn defer_collection() -> Result<ApplicationConfig> {
    let readiness = inspect()?;
    if !readiness.user_configured {
        return Err(eyre!(
            "Configure a diagnostic user before deferring the diagnostic source"
        ));
    }
    match readiness.workflow {
        Some(OnboardingWorkflow::CollectOnly) => {}
        Some(OnboardingWorkflow::CollectAndProcess)
            if readiness.output_configured
                && (readiness.output_from_environment
                    || !readiness.output_requires_keystore
                    || readiness.keystore_ready) => {}
        Some(OnboardingWorkflow::CollectAndProcess) => {
            return Err(eyre!(
                "Configure the diagnostic output before deferring the diagnostic source"
            ));
        }
        Some(OnboardingWorkflow::ProcessExisting) | None => {
            return Err(eyre!("The selected workflow does not use a diagnostic source"));
        }
    }
    if readiness.collect_host_configured {
        return Err(eyre!("A diagnostic source is already configured"));
    }

    let mut config = ApplicationConfig::load()?;
    config.collection_deferred = true;
    config.save()?;
    Ok(config)
}

/// Persists one linked Elasticsearch/Kibana deployment after the caller has
/// completed any remote validation it requires.
pub fn save_output_deployment(input: OutputDeploymentInput, keystore_password: &str) -> Result<ApplicationConfig> {
    validate_name(&input.output_name, "output host")?;
    validate_name(&input.viewer_name, "Kibana viewer")?;
    validate_name(&input.secret_id, "secret")?;
    if input.output_name == input.viewer_name {
        return Err(eyre!("Output host and Kibana viewer names must differ"));
    }

    upsert_secret_auth(&input.secret_id, input.auth.clone(), keystore_password)?;
    let output = KnownHostBuilder::new(input.output_url)
        .application(Application::Elasticsearch)
        .roles(vec![HostRole::Send])
        .viewer(Some(input.viewer_name.clone()))
        .secret(Some(input.secret_id.clone()))
        .build_with_secret_auth(input.auth.clone())?;
    let viewer = KnownHostBuilder::new(input.viewer_url)
        .application(Application::Kibana)
        .roles(vec![HostRole::View])
        .secret(Some(input.secret_id))
        .build_with_secret_auth(input.auth)?;

    let mut hosts = KnownHost::parse_hosts_yml()?;
    hosts.insert(input.output_name.clone(), output);
    hosts.insert(input.viewer_name, viewer);
    KnownHost::write_hosts_yml(&hosts)?;

    let mut config = ApplicationConfig::load()?;
    config.output.default = Some(input.output_name);
    config.save()?;
    Ok(config)
}

pub fn save_collect_host(input: CollectHostInput, keystore_password: Option<&str>) -> Result<()> {
    validate_name(&input.name, "collect host")?;
    let mut hosts = KnownHost::parse_hosts_yml()?;
    if let Some(existing) = hosts.get(&input.name).cloned() {
        hosts.insert(input.name, existing.with_role(HostRole::Collect));
        KnownHost::write_hosts_yml(&hosts)?;
        clear_collection_deferral()?;
        return Ok(());
    }

    match (&input.secret_id, &input.auth, keystore_password) {
        (Some(secret_id), Some(auth), Some(password)) => upsert_secret_auth(secret_id, auth.clone(), password)?,
        (Some(_), None, _) | (None, None, _) => {}
        _ => {
            return Err(eyre!(
                "Collect-host credentials require a secret id, authentication value, and unlocked keystore"
            ));
        }
    }

    let builder = KnownHostBuilder::new(input.url)
        .application(input.app)
        .roles(vec![HostRole::Collect])
        .secret(input.secret_id);
    let host = match input.auth {
        Some(auth) => builder.build_with_secret_auth(auth)?,
        None => builder.build()?,
    };
    hosts.insert(input.name, host);
    KnownHost::write_hosts_yml(&hosts)?;
    clear_collection_deferral()?;
    Ok(())
}

/// Replaces a collection host after the caller has obtained explicit user
/// confirmation. Unlike [`save_collect_host`], this updates the endpoint and
/// credential reference instead of only adding the collect role.
pub fn replace_collect_host(input: CollectHostInput, keystore_password: Option<&str>) -> Result<()> {
    validate_name(&input.name, "collect host")?;
    match (&input.secret_id, &input.auth, keystore_password) {
        (Some(secret_id), Some(auth), Some(password)) => upsert_secret_auth(secret_id, auth.clone(), password)?,
        (Some(_), None, _) | (None, None, _) => {}
        _ => {
            return Err(eyre!(
                "Collect-host credentials require a secret id, authentication value, and unlocked keystore"
            ));
        }
    }

    let builder = KnownHostBuilder::new(input.url)
        .application(input.app)
        .roles(vec![HostRole::Collect])
        .secret(input.secret_id);
    let host = match input.auth {
        Some(auth) => builder.build_with_secret_auth(auth)?,
        None => builder.build()?,
    };
    let mut hosts = KnownHost::parse_hosts_yml()?;
    hosts.insert(input.name, host);
    KnownHost::write_hosts_yml(&hosts)?;
    clear_collection_deferral()?;
    Ok(())
}

pub fn clear_collection_deferral() -> Result<()> {
    let mut config = ApplicationConfig::load()?;
    if config.collection_deferred {
        config.collection_deferred = false;
        config.save()?;
    }
    Ok(())
}

pub fn save_default_processing_job(name: String, collect_host: String) -> Result<ApplicationConfig> {
    validate_name(&name, "default job")?;
    let collect = KnownHost::get_known(&collect_host)
        .ok_or_else(|| eyre!("Configured collect host '{collect_host}' was not found"))?;
    if !collect.has_role(HostRole::Collect) {
        return Err(eyre!(
            "Configured collect host '{collect_host}' must include role 'collect'"
        ));
    }
    let config = ApplicationConfig::load()?;
    let output = if runtime_output_is_declared() {
        JobOutput::Environment
    } else {
        JobOutput::KnownHost {
            name: config
                .output
                .default
                .clone()
                .ok_or_else(|| eyre!("A validated output deployment is required before creating a default job"))?,
        }
    };
    let job = Job::builder().collect_from(collect_host)?.process_to(output)?;
    save_default_job(name, job)
}

pub fn save_default_job(name: String, job: Job) -> Result<ApplicationConfig> {
    validate_name(&name, "default job")?;
    let mut jobs: SavedJobs = crate::data::load_saved_jobs()?;
    jobs.insert(name.clone(), job);
    save_saved_jobs(&jobs)?;

    let mut config = ApplicationConfig::load()?;
    config.job.default = Some(name);
    config.validate_references()?;
    config.save()?;
    Ok(config)
}

fn valid_output_host(host: &KnownHost) -> bool {
    host.app() == Some(Application::Elasticsearch)
        && host.has_role(HostRole::Send)
        && host
            .viewer()
            .and_then(|name| KnownHost::get_known(&name.to_string()))
            .is_some_and(|viewer| viewer.app() == Some(Application::Kibana) && viewer.has_role(HostRole::View))
}

fn validate_name(name: &str, kind: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(eyre!("{kind} name cannot be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn transient_validation_uses_unsaved_credentials_without_reading_or_writing_keystore() {
        let _env = crate::TestEnv::new();
        for app in [
            crate::data::Application::Elasticsearch,
            crate::data::Application::Kibana,
        ] {
            let host = super::validation_host(
                url::Url::parse("http://127.0.0.1:9200").unwrap(),
                app,
                crate::data::SecretAuth::apikey("new-unsaved-key"),
            )
            .unwrap();
            assert!(host.secret.is_none());
            let serialized = yaml_serde::to_string(&host).unwrap();
            assert!(!serialized.contains("new-unsaved-key"));
            let uri = crate::data::Uri::try_from(host).unwrap();
            assert!(crate::client::Client::try_from(uri).is_ok());
            assert!(!crate::data::keystore_exists().unwrap());
        }
    }

    use super::{
        CollectHostInput, OnboardingReadiness, OutputDeploymentInput, defer_collection, inspect, replace_collect_host,
        save_collect_host, save_default_job, save_default_processing_job, save_output_deployment, save_user,
        save_workflow,
    };
    use crate::data::{
        Application, ApplicationConfig, HostRole, Job, KnownHost, KnownHostBuilder, OnboardingWorkflow, SecretAuth,
        create_keystore, resolve_secret_auth, upsert_secret_auth,
    };
    use std::collections::BTreeMap;
    use url::Url;

    #[test]
    fn onboarding_operations_persist_a_linked_output_and_collect_host() {
        let _env = crate::TestEnv::new();
        create_keystore("pw").expect("create keystore");
        save_output_deployment(
            OutputDeploymentInput {
                output_name: "output-es".to_string(),
                output_url: Url::parse("https://es.example:9200").expect("url"),
                viewer_name: "output-kibana".to_string(),
                viewer_url: Url::parse("https://kb.example:5601").expect("url"),
                secret_id: "output-auth".to_string(),
                auth: SecretAuth::apikey("secret"),
            },
            "pw",
        )
        .expect("save output");
        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");

        let config = ApplicationConfig::load().expect("load config");
        let readiness = inspect().expect("inspect readiness");

        assert_eq!(config.output.default.as_deref(), Some("output-es"));
        assert!(readiness.keystore_ready);
        assert!(readiness.output_configured);
        assert!(readiness.collect_host_configured);
        assert!(!OnboardingReadiness::is_complete(&readiness));
    }

    #[test]
    fn readiness_resumes_after_independently_persisted_stages() {
        let _env = crate::TestEnv::new();
        create_keystore("pw").expect("create keystore");
        save_output_deployment(
            OutputDeploymentInput {
                output_name: "output-es".to_string(),
                output_url: Url::parse("https://es.example:9200").expect("url"),
                viewer_name: "output-kibana".to_string(),
                viewer_url: Url::parse("https://kb.example:5601").expect("url"),
                secret_id: "output-auth".to_string(),
                auth: SecretAuth::apikey("secret"),
            },
            "pw",
        )
        .expect("save output");
        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");

        assert!(!inspect().expect("partial readiness").is_complete());

        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectAndProcess).expect("save workflow");
        save_default_processing_job("default".to_string(), "collect-es".to_string()).expect("save default job");

        assert!(inspect().expect("completed readiness").is_complete());
    }

    #[test]
    fn readiness_matches_the_selected_workflow() {
        let _env = crate::TestEnv::new();
        save_user("operator@example.com".to_string()).expect("save user");

        save_workflow(OnboardingWorkflow::ProcessExisting).expect("save process-existing workflow");
        assert!(!inspect().expect("missing output").is_complete());

        create_keystore("pw").expect("create keystore");
        save_output_deployment(
            OutputDeploymentInput {
                output_name: "output-es".to_string(),
                output_url: Url::parse("https://es.example:9200").expect("url"),
                viewer_name: "output-kibana".to_string(),
                viewer_url: Url::parse("https://kb.example:5601").expect("url"),
                secret_id: "output-auth".to_string(),
                auth: SecretAuth::apikey("secret"),
            },
            "pw",
        )
        .expect("save output");
        assert!(inspect().expect("process-existing readiness").is_complete());

        save_workflow(OnboardingWorkflow::CollectAndProcess).expect("save collect-and-process workflow");
        assert!(!inspect().expect("missing collection readiness").is_complete());

        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");
        save_default_processing_job("default".to_string(), "collect-es".to_string()).expect("save default job");
        assert!(inspect().expect("collect-and-process readiness").is_complete());
    }

    #[test]
    fn collection_can_be_deferred_without_marking_the_workflow_complete() {
        let _env = crate::TestEnv::new();
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save collect-only workflow");

        let initial = inspect().expect("initial readiness");
        assert!(!initial.is_complete());
        assert!(!initial.can_enter_application());

        defer_collection().expect("defer collection");

        let deferred = inspect().expect("deferred readiness");
        assert!(deferred.collection_deferred);
        assert!(!deferred.is_complete());
        assert!(deferred.can_enter_application());
    }

    #[test]
    fn deferred_processing_requires_a_usable_output() {
        let deferred = OnboardingReadiness {
            user_configured: true,
            workflow: Some(OnboardingWorkflow::CollectAndProcess),
            output_configured: true,
            output_requires_keystore: true,
            collection_deferred: true,
            ..OnboardingReadiness::default()
        };
        assert!(!deferred.can_enter_application());

        let unlocked = OnboardingReadiness {
            keystore_ready: true,
            ..deferred.clone()
        };
        assert!(unlocked.can_enter_application());

        let no_auth = OnboardingReadiness {
            output_requires_keystore: false,
            ..deferred.clone()
        };
        assert!(no_auth.can_enter_application());

        let environment = OnboardingReadiness {
            output_from_environment: true,
            ..deferred
        };
        assert!(environment.can_enter_application());
    }

    #[test]
    fn partial_environment_output_does_not_complete_readiness() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::ProcessExisting).expect("save workflow");

        let readiness = inspect().expect("inspect partial environment");
        assert!(!readiness.output_configured);
        assert!(!readiness.output_from_environment);
    }

    #[test]
    fn saving_a_collection_source_clears_collection_deferral() {
        let _env = crate::TestEnv::new();
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save collect-only workflow");
        defer_collection().expect("defer collection");

        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");

        let readiness = inspect().expect("saved source readiness");
        assert!(!readiness.collection_deferred);
        assert!(!readiness.can_enter_application());
    }

    #[test]
    fn changing_workflow_clears_collection_deferral() {
        let _env = crate::TestEnv::new();
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save collect-only workflow");
        defer_collection().expect("defer collection");

        save_workflow(OnboardingWorkflow::CollectAndProcess).expect("change workflow");

        assert!(!inspect().expect("changed readiness").collection_deferred);
    }

    #[test]
    fn collect_only_readiness_does_not_require_an_output() {
        let _env = crate::TestEnv::new();
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::CollectOnly).expect("save collect-only workflow");
        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");
        save_default_job(
            "default".to_string(),
            Job::builder()
                .collect_from("collect-es".to_string())
                .expect("collect source")
                .collect_to("diagnostics/collect-es".to_string())
                .expect("collect destination"),
        )
        .expect("save default job");

        let readiness = inspect().expect("collect-only readiness");
        assert!(!readiness.output_configured);
        assert!(readiness.is_complete());
    }

    #[test]
    fn environment_output_completes_process_existing_without_a_keystore() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-key");
        env.set("ESDIAG_KIBANA_URL", "https://kibana.example:5601");
        save_user("operator@example.com".to_string()).expect("save user");
        save_workflow(OnboardingWorkflow::ProcessExisting).expect("save workflow");

        let readiness = inspect().expect("environment readiness");
        assert!(readiness.output_from_environment);
        assert!(readiness.output_configured);
        assert!(!readiness.keystore_ready);
        assert!(readiness.is_complete());
    }

    #[test]
    fn processing_job_can_persist_the_environment_output() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://output.example:9200");
        env.set("ESDIAG_OUTPUT_APIKEY", "runtime-key");
        env.set("ESDIAG_KIBANA_URL", "https://kibana.example:5601");
        save_collect_host(
            CollectHostInput {
                name: "collect-es".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://collect.example:9200").expect("url"),
                secret_id: None,
                auth: None,
            },
            None,
        )
        .expect("save collect host");

        save_default_processing_job("default".to_string(), "collect-es".to_string()).expect("save environment job");

        let jobs = crate::data::load_saved_jobs().expect("load saved jobs");
        assert!(matches!(
            jobs["default"].process().map(|process| &process.export),
            Some(crate::data::JobOutput::Environment)
        ));
    }

    #[test]
    fn existing_collect_host_does_not_update_unrelated_credentials() {
        let _env = crate::TestEnv::new();
        create_keystore("pw").expect("create keystore");
        upsert_secret_auth("existing-secret", SecretAuth::apikey("existing-key"), "pw").expect("save secret");
        let existing = KnownHostBuilder::new(Url::parse("https://existing.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .secret(Some("existing-secret".to_string()))
            .build()
            .expect("host");
        KnownHost::write_hosts_yml(&BTreeMap::from([("existing".to_string(), existing)])).expect("write host");

        save_collect_host(
            CollectHostInput {
                name: "existing".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://replacement.example:9200").expect("url"),
                secret_id: Some("replacement-secret".to_string()),
                auth: Some(SecretAuth::apikey("replacement-key")),
            },
            Some("pw"),
        )
        .expect("mark existing host collectable");

        let host = KnownHost::get_known(&"existing".to_string()).expect("existing host");
        assert!(host.has_role(HostRole::Collect));
        assert_eq!(host.secret_reference(), Some("existing-secret"));
        assert_eq!(
            host.concrete_url().map(Url::as_str),
            Some("https://existing.example:9200/")
        );
        assert_eq!(
            resolve_secret_auth("existing-secret", "pw").expect("resolve existing secret"),
            Some(SecretAuth::apikey("existing-key"))
        );
        assert_eq!(
            resolve_secret_auth("replacement-secret", "pw").expect("resolve replacement secret"),
            None
        );
    }

    #[test]
    fn confirmed_collect_host_replacement_updates_endpoint_and_credentials() {
        let _env = crate::TestEnv::new();
        create_keystore("pw").expect("create keystore");
        let existing = KnownHostBuilder::new(Url::parse("https://existing.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .build()
            .expect("host");
        KnownHost::write_hosts_yml(&BTreeMap::from([("existing".to_string(), existing)])).expect("write host");

        replace_collect_host(
            CollectHostInput {
                name: "existing".to_string(),
                app: Application::Elasticsearch,
                url: Url::parse("https://replacement.example:9200").expect("url"),
                secret_id: Some("replacement-secret".to_string()),
                auth: Some(SecretAuth::apikey("replacement-key")),
            },
            Some("pw"),
        )
        .expect("replace collection host");

        let host = KnownHost::get_known(&"existing".to_string()).expect("replaced host");
        assert!(host.has_role(HostRole::Collect));
        assert_eq!(host.secret_reference(), Some("replacement-secret"));
        assert_eq!(
            host.concrete_url().map(Url::as_str),
            Some("https://replacement.example:9200/")
        );
        assert_eq!(
            resolve_secret_auth("replacement-secret", "pw").expect("resolve replacement secret"),
            Some(SecretAuth::apikey("replacement-key"))
        );
    }
}
