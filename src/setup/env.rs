// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::pipeline::{KibanaTarget, StackTargets};
use crate::client::Client;
use crate::data::{Application, HostRole, KnownHostBuilder, Uri, output_auth_from};
use eyre::{Result, WrapErr, eyre};
use std::collections::HashMap;
use std::path::Path;
use url::Url;

/// The variables `esdiag setup env` reads; every other key in an env file is ignored.
const SETUP_ENV_KEYS: [&str; 7] = [
    "ESDIAG_OUTPUT_URL",
    "ESDIAG_OUTPUT_APIKEY",
    "ESDIAG_OUTPUT_USERNAME",
    "ESDIAG_OUTPUT_PASSWORD",
    "ESDIAG_KIBANA_URL",
    "ESDIAG_KIBANA_SPACE",
    "ESDIAG_SETUP_ACTIVATE_TRIAL",
];

/// The deployment declared by the process environment and an optional env file.
pub struct EnvSetup {
    pub targets: StackTargets,
    pub notices: Vec<String>,
    /// `ESDIAG_SETUP_ACTIVATE_TRIAL` from the env file; the process value is read by the caller.
    pub activate_trial: Option<String>,
}

impl EnvSetup {
    /// Process environment values take precedence over `env_file`, which is parsed without
    /// changing the process environment.
    pub fn load(env_file: Option<&Path>) -> Result<Self> {
        let file = match env_file {
            Some(path) => read_env_file(path)?,
            None => HashMap::new(),
        };
        Self::from_lookup(|name| declared_value(&file, name), &file)
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>, file: &HashMap<String, String>) -> Result<Self> {
        let declared = |name: &str| lookup(name).filter(|value| !value.trim().is_empty());
        let output_url = declared("ESDIAG_OUTPUT_URL");
        let kibana_url = declared("ESDIAG_KIBANA_URL");
        if output_url.is_none() && kibana_url.is_none() {
            return Err(eyre!(
                "Neither ESDIAG_OUTPUT_URL nor ESDIAG_KIBANA_URL is declared. Set them in the environment or pass --env-file <PATH>."
            ));
        }
        let auth = output_auth_from(&lookup)?;
        let host = |url: &str, application, role| {
            KnownHostBuilder::new(Url::parse(url).wrap_err_with(|| format!("Invalid {application} URL"))?)
                .application(application)
                .roles(vec![role])
                .auth(auth.clone())
                .build()
        };

        let mut notices = Vec::new();
        let elasticsearch = match &output_url {
            Some(url) => Some(Client::try_from(Uri::try_from(host(
                url,
                Application::Elasticsearch,
                HostRole::Send,
            )?)?)?),
            None => {
                notices.push("Elasticsearch setup was skipped because ESDIAG_OUTPUT_URL is not declared.".to_string());
                None
            }
        };
        let kibana = match &kibana_url {
            Some(url) => Some(KibanaTarget::from_host(
                host(url, Application::Kibana, HostRole::View)?,
                lookup("ESDIAG_KIBANA_SPACE").as_deref(),
            )?),
            None => {
                notices.push("Kibana setup was skipped because ESDIAG_KIBANA_URL is not declared.".to_string());
                None
            }
        };
        Ok(Self {
            targets: StackTargets { elasticsearch, kibana },
            notices,
            activate_trial: file.get("ESDIAG_SETUP_ACTIVATE_TRIAL").cloned(),
        })
    }
}

fn declared_value(file: &HashMap<String, String>, name: &str) -> Option<String> {
    std::env::var(name).ok().or_else(|| file.get(name).cloned())
}

fn read_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let entries =
        dotenvy::from_path_iter(path).wrap_err_with(|| format!("Failed to read env file {}", path.display()))?;
    let mut values = HashMap::new();
    for entry in entries {
        let (key, value) = entry.wrap_err_with(|| format!("Failed to parse env file {}", path.display()))?;
        if SETUP_ENV_KEYS.contains(&key.as_str()) {
            values.insert(key, value);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::{EnvSetup, declared_value, read_env_file};
    use std::collections::HashMap;

    fn load(process: &[(&str, &str)], file: &[(&str, &str)]) -> eyre::Result<EnvSetup> {
        let process: HashMap<String, String> = process.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let file: HashMap<String, String> = file.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        EnvSetup::from_lookup(|name| process.get(name).or_else(|| file.get(name)).cloned(), &file)
    }

    #[test]
    fn process_environment_overrides_the_file() {
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_OUTPUT_URL", "https://env.example:9200");
        env.remove("ESDIAG_KIBANA_URL");
        let file = HashMap::from([
            ("ESDIAG_OUTPUT_URL".to_string(), "https://file.example:9200".to_string()),
            (
                "ESDIAG_KIBANA_URL".to_string(),
                "https://file-kb.example:5601".to_string(),
            ),
        ]);
        assert_eq!(
            declared_value(&file, "ESDIAG_OUTPUT_URL").as_deref(),
            Some("https://env.example:9200")
        );
        assert_eq!(
            declared_value(&file, "ESDIAG_KIBANA_URL").as_deref(),
            Some("https://file-kb.example:5601")
        );
    }

    #[test]
    fn file_only_declares_both_services_and_the_trial() {
        let setup = load(
            &[],
            &[
                ("ESDIAG_OUTPUT_URL", "https://es.example:9200"),
                ("ESDIAG_KIBANA_URL", "https://kb.example:5601"),
                ("ESDIAG_OUTPUT_APIKEY", "key"),
                ("ESDIAG_SETUP_ACTIVATE_TRIAL", "true"),
            ],
        )
        .unwrap();
        assert!(setup.targets.elasticsearch.is_some() && setup.targets.kibana.is_some());
        assert!(setup.notices.is_empty());
        assert_eq!(setup.activate_trial.as_deref(), Some("true"));
    }

    #[test]
    fn single_service_declarations_add_notices() {
        let setup = load(&[("ESDIAG_OUTPUT_URL", "https://es.example:9200")], &[]).unwrap();
        assert!(setup.targets.kibana.is_none());
        assert!(setup.notices[0].contains("ESDIAG_KIBANA_URL"));

        let setup = load(&[("ESDIAG_KIBANA_URL", "https://kb.example:5601")], &[]).unwrap();
        assert!(setup.targets.elasticsearch.is_none());
        assert!(setup.notices[0].contains("ESDIAG_OUTPUT_URL"));
    }

    #[test]
    fn kibana_space_follows_the_declared_setting() {
        let setup = load(
            &[
                ("ESDIAG_KIBANA_URL", "https://kb.example:5601/s/ops"),
                ("ESDIAG_KIBANA_SPACE", "support"),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(setup.targets.kibana.unwrap().space.as_deref(), Some("support"));

        let setup = load(
            &[("ESDIAG_KIBANA_URL", "https://kb.example:5601")],
            &[("ESDIAG_KIBANA_SPACE", "_default")],
        )
        .unwrap();
        assert_eq!(setup.targets.kibana.unwrap().space, None);

        let setup = load(&[("ESDIAG_KIBANA_URL", "https://kb.example:5601/s/ops")], &[]).unwrap();
        assert_eq!(setup.targets.kibana.unwrap().space.as_deref(), Some("esdiag"));
    }

    #[test]
    fn rejects_missing_endpoints_and_conflicting_auth() {
        let error = load(&[], &[]).err().unwrap().to_string();
        assert!(
            error.contains("ESDIAG_OUTPUT_URL") && error.contains("ESDIAG_KIBANA_URL") && error.contains("--env-file")
        );

        let error = load(
            &[
                ("ESDIAG_OUTPUT_URL", "https://es.example:9200"),
                ("ESDIAG_OUTPUT_APIKEY", "key"),
                ("ESDIAG_OUTPUT_USERNAME", "elastic"),
            ],
            &[],
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("cannot be combined"));
    }

    #[test]
    fn env_file_ignores_unrelated_keys_and_keeps_the_process_environment() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("deploy.env");
        std::fs::write(
            &path,
            "ESDIAG_OUTPUT_URL=https://file.example:9200\nESDIAG_UNRELATED_SETUP_KEY=1\nPATH=/nowhere\n",
        )
        .unwrap();
        let values = read_env_file(&path).unwrap();
        assert_eq!(values.len(), 1);
        assert!(std::env::var_os("ESDIAG_UNRELATED_SETUP_KEY").is_none());
        assert_ne!(std::env::var("PATH").ok().as_deref(), Some("/nowhere"));
    }
}
