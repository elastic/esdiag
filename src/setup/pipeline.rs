// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{AgentBuilderLicense, SetupReport, activate_trial_license, agent_builder_license_status};
use super::{assets_report, kibana_assets_report};
use crate::client::Client;
use crate::data::{KnownHost, Uri};
use eyre::{Result, eyre};

/// A Kibana endpoint and the space ESDiag assets are installed into; `None` is the default space.
pub struct KibanaTarget {
    pub client: Client,
    pub space: Option<String>,
}

impl KibanaTarget {
    /// Install into the `ESDIAG_KIBANA_SPACE` selection, the same space `process` and `serve` link to.
    pub fn from_host(host: KnownHost, declared_space: Option<&str>) -> Result<Self> {
        Ok(Self {
            client: Client::try_from(Uri::try_from(host)?)?,
            space: crate::env::kibana_space_setting(declared_space),
        })
    }
}

/// The services one setup source declares.
pub struct StackTargets {
    pub elasticsearch: Option<Client>,
    pub kibana: Option<KibanaTarget>,
}

/// The endpoints set up and the merged report across stages.
pub struct StackSetup {
    pub targets: Vec<String>,
    pub report: SetupReport,
}

pub const TRIAL_OPT_IN_WARNING: &str = "Kibana assets were skipped because the Elasticsearch license does not include \
     Agent Builder. Rerun with --activate-trial or ESDIAG_SETUP_ACTIVATE_TRIAL=true to start a 30-day trial, \
     or install an Enterprise license.";

/// Install Elasticsearch assets, check the license, then install Kibana assets.
///
/// The license is only read when both services are declared, and a trial is
/// only started when `activate_trial` is set and the license is inactive.
pub async fn stack(targets: &StackTargets, activate_trial: bool) -> Result<StackSetup> {
    let mut setup = StackSetup {
        targets: Vec::new(),
        report: SetupReport::default(),
    };
    if targets.elasticsearch.is_none() && targets.kibana.is_none() {
        return Err(eyre!("No Elasticsearch or Kibana endpoint was declared"));
    }

    if let Some(elasticsearch) = &targets.elasticsearch {
        tracing::info!("Setting up assets in {elasticsearch}");
        setup.report = assets_report(elasticsearch).await?;
        setup.targets.push(elasticsearch.to_string());

        if targets.kibana.is_some() {
            match agent_builder_license_status(elasticsearch).await? {
                AgentBuilderLicense::Active | AgentBuilderLicense::Managed => {}
                AgentBuilderLicense::Inactive if activate_trial => activate_trial_license(elasticsearch).await?,
                AgentBuilderLicense::Inactive => {
                    setup.report.warnings.push(TRIAL_OPT_IN_WARNING.to_string());
                    return Ok(setup);
                }
            }
        }
    }

    if let Some(kibana) = &targets.kibana {
        tracing::info!("Setting up Kibana assets in {}", kibana.client);
        let report = kibana_assets_report(&kibana.client, kibana.space.clone()).await?;
        setup.report.failed_indices.extend(report.failed_indices);
        setup.report.warnings.extend(report.warnings);
        setup.report.notices.extend(report.notices);
        setup.targets.push(kibana.client.to_string());
    }
    Ok(setup)
}
