// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::setup_outcome;
use crate::{
    cli_output::CommandOutcome,
    client::Client,
    data::{Application, ApplicationConfig, KnownHost, OutputDeployment, Uri},
    setup::{self, pipeline::KibanaTarget, pipeline::StackTargets},
};
use eyre::{Result, eyre};
use std::path::PathBuf;

/// Installs Elasticsearch assets into a saved host, or the persisted output when `host` is `None`.
pub async fn setup_elasticsearch(host: Option<String>) -> Result<CommandOutcome> {
    let host = setup_service_host(host.as_deref(), Application::Elasticsearch)?;
    let client = Client::try_from(Uri::try_from(host)?)?;
    tracing::info!("Setting up assets in {client}");
    let report = setup::assets_report(&client).await?;
    Ok(CommandOutcome::Completed(setup_outcome(
        vec![client.to_string()],
        report,
    )))
}

/// Installs Kibana assets into a saved host, or the persisted output's viewer when `host` is `None`.
pub async fn setup_kibana(host: Option<String>) -> Result<CommandOutcome> {
    let host = setup_service_host(host.as_deref(), Application::Kibana)?;
    let kibana = KibanaTarget::from_host(host, process_kibana_space().as_deref())?;
    tracing::info!("Setting up Kibana assets in {}", kibana.client);
    let report = setup::kibana_assets_report(&kibana.client, kibana.space).await?;
    Ok(CommandOutcome::Completed(setup_outcome(
        vec![kibana.client.to_string()],
        report,
    )))
}

/// Installs assets into a saved output and its linked Kibana viewer.
pub async fn setup_output(name: Option<String>, activate_trial: bool) -> Result<CommandOutcome> {
    let activate_trial = resolve_activate_trial(activate_trial, None)?;
    let deployment = OutputDeployment::resolve_saved(name.as_deref(), true)?;
    let output_name = name
        .or_else(|| ApplicationConfig::load().ok()?.output.default)
        .unwrap_or_else(|| "output".to_string());
    let kibana = deployment
        .kibana
        .map(|host| KibanaTarget::from_host(host, process_kibana_space().as_deref()))
        .transpose()?;
    let notice = kibana
        .is_none()
        .then(|| format!("Kibana setup was skipped because output '{output_name}' has no linked Kibana viewer."));
    let targets = StackTargets {
        elasticsearch: Some(Client::try_from(Uri::try_from(deployment.elasticsearch)?)?),
        kibana,
    };
    run_stack_setup(targets, notice.into_iter().collect(), activate_trial).await
}

/// Installs assets into the deployment declared by `ESDIAG_OUTPUT_*` and `ESDIAG_KIBANA_*`,
/// read from `env_file` when given; the process environment takes precedence.
pub async fn setup_env(env_file: Option<PathBuf>, activate_trial: bool) -> Result<CommandOutcome> {
    let declared = setup::env::EnvSetup::load(env_file.as_deref())?;
    let activate_trial = resolve_activate_trial(activate_trial, declared.activate_trial.as_deref())?;
    run_stack_setup(declared.targets, declared.notices, activate_trial).await
}

/// Installs assets into the services of an Elastic CLI context.
pub async fn setup_context(
    name: Option<String>,
    config: Option<PathBuf>,
    activate_trial: bool,
) -> Result<CommandOutcome> {
    let activate_trial = resolve_activate_trial(activate_trial, None)?;
    let declared = setup::context::ContextSetup::load(name.as_deref(), config.as_deref())?;
    run_stack_setup(declared.targets, declared.notices, activate_trial).await
}

/// The `--activate-trial` flag wins; otherwise `ESDIAG_SETUP_ACTIVATE_TRIAL`, then a value
/// declared by the setup source.
pub fn resolve_activate_trial(flag: bool, declared: Option<&str>) -> Result<bool> {
    if flag {
        return Ok(true);
    }
    match std::env::var("ESDIAG_SETUP_ACTIVATE_TRIAL")
        .ok()
        .as_deref()
        .or(declared)
    {
        None => Ok(false),
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Ok(true),
            "0" | "false" | "no" | "n" | "off" | "" => Ok(false),
            _ => Err(eyre!(
                "ESDIAG_SETUP_ACTIVATE_TRIAL must be true or false, got {value:?}"
            )),
        },
    }
}

pub fn process_kibana_space() -> Option<String> {
    std::env::var("ESDIAG_KIBANA_SPACE").ok()
}

/// A saved host for single-service setup, or the persisted output's matching service.
fn setup_service_host(name: Option<&str>, app: Application) -> Result<KnownHost> {
    let subcommand = app.to_string().to_ascii_lowercase();
    let Some(name) = name else {
        return match app {
            Application::Kibana => OutputDeployment::persisted_viewer()?.ok_or_else(|| {
                eyre!(
                    "The persisted output has no linked Kibana viewer. Run `esdiag setup kibana <HOST>` with a saved Kibana host."
                )
            }),
            _ => Ok(OutputDeployment::resolve_saved(None, false)?.elasticsearch),
        };
    };
    let host = KnownHost::get_known(&name.to_string()).ok_or_else(|| {
        eyre!("No saved host is named '{name}'. Add it with `esdiag host add`, then run `esdiag setup {subcommand} {name}`.")
    })?;
    match host.app() {
        Some(found) if found == app => Ok(host),
        Some(found @ (Application::Elasticsearch | Application::Kibana)) => Err(eyre!(
            "Saved host '{name}' is a {found} host, not {app}. Run `esdiag setup {} {name}`.",
            found.to_string().to_ascii_lowercase()
        )),
        Some(found) => Err(eyre!(
            "Saved host '{name}' is a {found} host; `esdiag setup {subcommand}` needs a {app} host."
        )),
        None => Err(eyre!(
            "Saved host '{name}' has no application; `esdiag setup {subcommand}` needs a {app} host."
        )),
    }
}

async fn run_stack_setup(targets: StackTargets, notices: Vec<String>, activate_trial: bool) -> Result<CommandOutcome> {
    let mut setup = setup::pipeline::stack(&targets, activate_trial).await?;
    setup.report.notices.extend(notices);
    Ok(CommandOutcome::Completed(setup_outcome(setup.targets, setup.report)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TestEnv,
        data::{HostRole, KnownHostBuilder},
    };
    use std::collections::BTreeMap;
    use url::Url;

    #[test]
    fn trial_opt_in_prefers_the_flag_then_the_process_environment() {
        let mut env = TestEnv::new();
        env.remove("ESDIAG_SETUP_ACTIVATE_TRIAL");
        assert!(!resolve_activate_trial(false, None).unwrap());
        assert!(resolve_activate_trial(false, Some("true")).unwrap());
        assert!(resolve_activate_trial(true, Some("false")).unwrap());
        env.set("ESDIAG_SETUP_ACTIVATE_TRIAL", "false");
        assert!(!resolve_activate_trial(false, Some("true")).unwrap());
        env.set("ESDIAG_SETUP_ACTIVATE_TRIAL", "sometimes");
        assert!(resolve_activate_trial(false, None).is_err());
    }

    #[test]
    fn setup_service_host_checks_the_saved_application() {
        let _env = TestEnv::new();
        let kibana = KnownHostBuilder::new(Url::parse("https://kb.example:5601").unwrap())
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .unwrap();
        KnownHost::write_hosts_yml(&BTreeMap::from([("diagnostics-kb".to_string(), kibana)])).unwrap();
        let error = setup_service_host(Some("diagnostics-kb"), Application::Elasticsearch)
            .unwrap_err()
            .to_string();
        assert!(error.contains("esdiag setup kibana diagnostics-kb"), "{error}");
        assert!(setup_service_host(Some("diagnostics-kb"), Application::Kibana).is_ok());
        assert!(setup_service_host(Some("missing"), Application::Kibana).is_err());
    }

    #[test]
    fn setup_kibana_fails_when_the_persisted_output_has_no_viewer() {
        let _env = TestEnv::new();
        let output = KnownHostBuilder::new(Url::parse("https://es.example:9200").unwrap())
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .build()
            .unwrap();
        KnownHost::write_hosts_yml(&BTreeMap::from([("out".to_string(), output)])).unwrap();
        let mut config = ApplicationConfig::new();
        config.output.default = Some("out".to_string());
        config.save().unwrap();

        let error = setup_service_host(None, Application::Kibana).unwrap_err().to_string();
        assert!(error.contains("no linked Kibana viewer"), "{error}");
        assert!(setup_service_host(None, Application::Elasticsearch).is_ok());
    }

    #[test]
    fn setup_elasticsearch_ignores_an_unavailable_viewer_credential() {
        let _env = TestEnv::new();
        let output = KnownHostBuilder::new(Url::parse("https://es.example:9200").unwrap())
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .viewer(Some("locked-kb".to_string()))
            .build()
            .unwrap();
        let viewer = KnownHostBuilder::new(Url::parse("https://kb.example:5601").unwrap())
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .secret(Some("missing-secret".to_string()))
            .build()
            .unwrap();
        KnownHost::write_hosts_yml(&BTreeMap::from([
            ("out".to_string(), output),
            ("locked-kb".to_string(), viewer),
        ]))
        .unwrap();
        let mut config = ApplicationConfig::new();
        config.output.default = Some("out".to_string());
        config.save().unwrap();

        assert!(setup_service_host(None, Application::Elasticsearch).is_ok());
        let error = setup_service_host(None, Application::Kibana).unwrap_err().to_string();
        assert!(error.contains("missing-secret"), "{error}");
    }

    #[test]
    fn setup_kibana_ignores_an_unavailable_output_credential() {
        let _env = TestEnv::new();
        let output = KnownHostBuilder::new(Url::parse("https://es.example:9200").unwrap())
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .secret(Some("missing-secret".to_string()))
            .viewer(Some("kb".to_string()))
            .build()
            .unwrap();
        let viewer = KnownHostBuilder::new(Url::parse("https://kb.example:5601").unwrap())
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .unwrap();
        KnownHost::write_hosts_yml(&BTreeMap::from([
            ("out".to_string(), output),
            ("kb".to_string(), viewer),
        ]))
        .unwrap();
        let mut config = ApplicationConfig::new();
        config.output.default = Some("out".to_string());
        config.save().unwrap();

        let viewer = setup_service_host(None, Application::Kibana).expect("viewer without the output secret");
        assert_eq!(viewer.app(), Some(Application::Kibana));
        let error = setup_service_host(None, Application::Elasticsearch)
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing-secret"), "{error}");
    }
}
