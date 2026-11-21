// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::pipeline::{KibanaTarget, StackTargets};
use crate::client::Client;
use crate::data::{Application, HostRole, KnownHost, KnownHostBuilder, Uri};
use elasticrc::{Auth, ConfigFile, Context};
use eyre::{Result, WrapErr, eyre};
use std::path::Path;

/// The services of one Elastic CLI context, resolved in memory.
pub struct ContextSetup {
    pub name: String,
    pub targets: StackTargets,
    pub notices: Vec<String>,
}

impl ContextSetup {
    /// Load `config`, then `ELASTIC_CLI_CONFIG_FILE`, then the home config, and select
    /// `name` or the current context.
    pub fn load(name: Option<&str>, config: Option<&Path>) -> Result<Self> {
        let file =
            ConfigFile::load_with_options(config, None).wrap_err("Failed to load the Elastic CLI configuration")?;
        let name = name.unwrap_or_else(|| file.current_context_name()).to_string();
        let context = file.context(&name)?;
        Self::from_context(name, context, std::env::var("ESDIAG_KIBANA_SPACE").ok().as_deref())
    }

    fn from_context(name: String, context: &Context, declared_space: Option<&str>) -> Result<Self> {
        if context.elasticsearch.is_none() && context.kibana.is_none() {
            return Err(eyre!(
                "Context '{name}' defines neither an elasticsearch nor a kibana service"
            ));
        }
        let mut notices = Vec::new();
        let elasticsearch = match &context.elasticsearch {
            Some(service) => {
                let service = service.resolve()?;
                let host = known_host(service.url, service.auth, Application::Elasticsearch, HostRole::Send)?;
                Some(Client::try_from(Uri::try_from(host)?)?)
            }
            None => {
                notices.push(format!(
                    "Elasticsearch setup was skipped because context '{name}' defines no elasticsearch service."
                ));
                None
            }
        };
        let kibana = match &context.kibana {
            Some(service) => {
                let service = service.resolve()?;
                let host = known_host(service.url, service.auth, Application::Kibana, HostRole::View)?;
                Some(KibanaTarget::from_host(host, declared_space)?)
            }
            None => {
                notices.push(format!(
                    "Kibana setup was skipped because context '{name}' defines no kibana service."
                ));
                None
            }
        };
        Ok(Self {
            name,
            targets: StackTargets { elasticsearch, kibana },
            notices,
        })
    }
}

fn known_host(url: url::Url, auth: Auth, application: Application, role: HostRole) -> Result<KnownHost> {
    let auth = match auth {
        Auth::ApiKey(key) => crate::data::Auth::apikey(key.expose_secret().to_string()),
        Auth::Basic { username, password } => crate::data::Auth::basic(username, password.expose_secret().to_string()),
        Auth::None => crate::data::Auth::None,
    };
    KnownHostBuilder::new(url)
        .application(application)
        .roles(vec![role])
        .auth(auth)
        .build()
}

#[cfg(test)]
mod tests {
    use super::ContextSetup;
    use std::path::PathBuf;

    const CONFIG: &str = r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://es.example:9200
      auth:
        api_key: inline-es-key
    kibana:
      url: https://kb.example:5601/s/ops
      auth:
        api_key: inline-kb-key
  es-only:
    elasticsearch:
      url: https://es-only.example:9200
  kb-only:
    kibana:
      url: https://kb-only.example:5601
  cloud-only:
    cloud:
      url: https://api.elastic-cloud.com
  staging:
    elasticsearch:
      url: https://staging.example:9200
      auth:
        api_key: $(env:ESDIAG_CONTEXT_TEST_KEY)
"#;

    fn config() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join(".elasticrc.yml");
        std::fs::write(&path, CONFIG).unwrap();
        (directory, path)
    }

    #[test]
    fn current_context_resolves_both_services_in_the_selected_space() {
        let (_directory, path) = config();
        let mut env = crate::TestEnv::new();
        env.remove("ESDIAG_KIBANA_SPACE");
        let setup = ContextSetup::load(None, Some(&path)).unwrap();
        assert_eq!(setup.name, "prod");
        assert!(setup.targets.elasticsearch.is_some());
        assert_eq!(setup.targets.kibana.as_ref().unwrap().space.as_deref(), Some("esdiag"));
        assert!(setup.notices.is_empty());
    }

    #[test]
    fn partial_contexts_add_notices() {
        let (_directory, path) = config();
        let setup = ContextSetup::load(Some("es-only"), Some(&path)).unwrap();
        assert!(setup.targets.kibana.is_none());
        assert!(setup.notices[0].contains("'es-only' defines no kibana service"));

        let setup = ContextSetup::load(Some("kb-only"), Some(&path)).unwrap();
        assert!(setup.targets.elasticsearch.is_none());
        assert!(setup.notices[0].contains("defines no elasticsearch service"));
    }

    #[test]
    fn cloud_only_and_unknown_contexts_fail() {
        let (_directory, path) = config();
        let error = ContextSetup::load(Some("cloud-only"), Some(&path)).err().unwrap();
        assert!(error.to_string().contains("'cloud-only'"));

        let error = format!("{:#}", ContextSetup::load(Some("missing"), Some(&path)).err().unwrap());
        assert!(error.contains("missing"), "{error}");
        assert!(error.contains("prod") && error.contains("es-only"), "{error}");
    }

    #[test]
    fn env_resolver_supplies_the_key_without_exposing_it() {
        let (_directory, path) = config();
        let mut env = crate::TestEnv::new();
        env.set("ESDIAG_CONTEXT_TEST_KEY", "resolved-secret-key");
        let setup = ContextSetup::load(Some("staging"), Some(&path)).unwrap();
        let target = setup.targets.elasticsearch.unwrap().to_string();
        assert!(!target.contains("resolved-secret-key"), "{target}");
    }
}
