// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Elastic Stack application components (the application axis)
mod application;
/// Local non-secret workflow preferences
mod application_config;
/// Authentication methods
mod auth;
/// Encrypted secret storage
mod keystore;
/// Manage saving and loading hosts from a YAML file
mod known_host;
/// Canonical Elasticsearch and Kibana output-deployment resolution
mod output_deployment;
/// Deployment platforms (the platform axis)
mod platform;
/// Saved job configurations
pub mod saved_jobs;
/// Application Settings
pub mod settings;
/// Universal resource identifiers
mod uri;

pub use application::Application;
pub use application_config::{ApplicationConfig, JobConfig, OnboardingWorkflow, OutputConfig};
pub use auth::{Auth, AuthType};
#[cfg(all(feature = "server", feature = "keystore"))]
pub(crate) use keystore::get_active_unlock_keystore_password;
#[cfg(all(feature = "server", feature = "keystore"))]
pub(crate) use keystore::list_secret_entries;
pub use keystore::{
    BasicSecret, SecretAuth, SecretEntry, UnlockLease, UnlockStatus, add_secret, authenticate, clear_unlock_lease,
    create_keystore, default_unlock_ttl, get_keystore_password, get_keystore_path, get_password_for_secret_commands,
    get_secret, get_unlock_path, get_unlock_status, keystore_exists, list_secret_names, parse_unlock_ttl,
    read_unlock_lease, remove_secret, resolve_secret_auth, rotate_keystore_password, update_secret, upsert_secret_auth,
    validate_existing_keystore_password, with_scoped_keystore_password, write_unlock_lease,
};
#[cfg(all(test, feature = "server"))]
pub(crate) use known_host::write_hosts_yml_for_tests;
pub use known_host::{
    CredentialDirection, ElasticCloud, HostRole, HostRoute, KnownHost, KnownHostBuilder, KnownHostCliUpdate,
    ResolvedKnownHost,
};
pub use output_deployment::{OutputDeployment, OutputDeploymentSource};
pub use platform::Platform;
pub use saved_jobs::{
    CollectMode, CollectSource, DraftTargetAvailability, Job, JobBuilder, JobDraft, JobDraftCollect, JobDraftProcess,
    JobDraftSend, JobOutput, JobProcessSelection, JobSignals, JobSignalsCollect, JobSignalsProcess, JobSignalsSend,
    NeedsAction, NeedsCollect, ProcessMode, SavedJobs, SendMode, load_saved_jobs, load_saved_jobs_async,
    save_saved_jobs, with_saved_jobs_async,
};
pub use settings::Settings;
pub use uri::Uri;

use crate::env;
use eyre::{Result, eyre};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

pub fn collect_application(app: Option<Application>) -> Result<Application> {
    match app {
        Some(application @ (Application::Elasticsearch | Application::Kibana | Application::Logstash)) => {
            Ok(application)
        }
        Some(Application::Agent) => Err(eyre!(
            "Collect is out of scope by design for Elastic Agent. Elastic Agent provides its own diagnostic bundle; acquire it through CLI `process` input or Web UI `Upload`."
        )),
        None => Err(eyre!(
            "Collect is out of scope by design for platform targets (ECE, ECK, and KubernetesPlatform). Load the platform-generated bundle through CLI `process` input or Web UI `Upload`; only Elasticsearch, Kibana, and Logstash support API collection."
        )),
    }
}

pub fn is_collectable_app(app: Option<Application>) -> bool {
    matches!(
        app,
        Some(Application::Elasticsearch | Application::Kibana | Application::Logstash)
    )
}

/// Save an arbitrary serializable object to a file
pub fn save_file<T: Serialize>(filename: &str, content: &T) -> Result<()> {
    let home_file = PathBuf::from(env::get_string("HOME")?)
        .join(env::get_string("ESDIAG_HOME")?)
        .join("last_run")
        .join(filename);
    let mut file = OpenOptions::new().create(true).append(true).open(home_file)?;
    let body = serde_json::to_string(&content)?;
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

/// The standard deserializer from serde_json does not deserializing u64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.
pub fn u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_u64()),
        Value::String(s) => Ok(s.parse::<u64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

/// The standard deserializer from serde_json does not deserializing i64 from
/// strings. Unfortunately the _settings API frequently wraps numbers in quotes.
pub fn i64_from_string<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Number(num) => Ok(num.as_i64()),
        Value::String(s) => Ok(s.parse::<i64>().ok()),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom(
            "expected a number or a string representing a number",
        )),
    }
}

pub fn map_as_vec_entries<'de, D, T>(deserializer: D) -> Result<Vec<(String, T)>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for MapVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<(String, T)>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut values = Vec::with_capacity(map.size_hint().unwrap_or(0));
            while let Some(key) = map.next_key()? {
                let value = map.next_value()?;
                values.push((key, value));
            }
            Ok(values)
        }
    }

    deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
}

pub fn option_map_as_vec_entries<'de, D, T>(deserializer: D) -> Result<Option<Vec<(String, T)>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct OptionMapVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for OptionMapVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Option<Vec<(String, T)>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an optional map")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            map_as_vec_entries(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionMapVisitor(std::marker::PhantomData))
}

#[cfg(test)]
mod tests {
    use super::{Application, collect_application};

    #[test]
    fn collect_application_accepts_every_api_collectable_application() {
        for application in [Application::Elasticsearch, Application::Kibana, Application::Logstash] {
            assert_eq!(
                collect_application(Some(application)).expect("collectable application"),
                application
            );
        }
    }

    #[test]
    fn collect_application_refuses_non_collectable_targets_as_by_design() {
        for application in [Some(Application::Agent), None] {
            let error = collect_application(application)
                .expect_err("non-collectable target must be rejected")
                .to_string();
            assert!(error.contains("out of scope by design"));
            assert!(error.contains("CLI `process` input"));
            assert!(error.contains("Web UI `Upload`"));
            assert!(!error.contains("not yet implemented"));
        }
    }
}
