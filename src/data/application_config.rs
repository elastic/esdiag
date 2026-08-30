// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{Application, ElasticOutputContext, HostRole, KnownHost, load_saved_jobs};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const CURRENT_VERSION: u32 = 1;
const FILE_NAME: &str = "esdiag.yml";

/// Non-secret preferences shared by local ESDiag workflows.
///
/// Endpoint definitions remain in `hosts.yml`, saved workflow bodies remain in
/// `jobs.yml`, and credentials remain encrypted in `secrets.yml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<OnboardingWorkflow>,
    #[serde(default, skip_serializing_if = "OutputConfig::is_empty")]
    pub output: OutputConfig,
    #[serde(default, skip_serializing_if = "JobConfig::is_empty")]
    pub job: JobConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnboardingWorkflow {
    CollectOnly,
    ProcessExisting,
    CollectAndProcess,
}

impl OnboardingWorkflow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CollectOnly => "collection only",
            Self::ProcessExisting => "process existing diagnostics",
            Self::CollectAndProcess => "collect and process diagnostics",
        }
    }

    pub fn collects_diagnostics(self) -> bool {
        matches!(self, Self::CollectOnly | Self::CollectAndProcess)
    }

    pub fn processes_diagnostics(self) -> bool {
        matches!(self, Self::ProcessExisting | Self::CollectAndProcess)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elastic_context: Option<ElasticOutputContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_version: Option<String>,
}

impl OutputConfig {
    fn is_empty(&self) -> bool {
        self.default.is_none()
            && self.elastic_context.is_none()
            && self.authenticated_on.is_none()
            && self.assets_version.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl JobConfig {
    fn is_empty(&self) -> bool {
        self.default.is_none()
    }
}

impl ApplicationConfig {
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            ..Self::default()
        }
    }

    pub fn path() -> Result<PathBuf> {
        let hosts_path = KnownHost::get_hosts_path();
        let state_dir = hosts_path.parent().unwrap_or_else(|| std::path::Path::new("."));
        Ok(state_dir.join(FILE_NAME))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&path)?;
        let config: Self = yaml_serde::from_str(&content)?;
        config.validate_version()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        self.validate_version()?;
        let path = Self::path()?;
        super::keystore::write_yaml_atomic(&path, self)
    }

    /// Checks that persisted references still form a valid local workflow
    /// without resolving any credential material.
    pub fn validate_references(&self) -> Result<()> {
        self.validate_version()?;

        let hosts = KnownHost::parse_hosts_yml()?;
        if let Some(output_context) = &self.output.elastic_context {
            #[cfg(feature = "elasticrc")]
            {
                let config = elasticrc::ConfigFile::load_with_options(output_context.config_file.as_deref(), None)?;
                let context = config.contexts.get(&output_context.context).ok_or_else(|| {
                    eyre!(
                        "Configured Elastic CLI output context '{}' was not found",
                        output_context.context
                    )
                })?;
                if context.elasticsearch.is_none() {
                    return Err(eyre!(
                        "Configured Elastic CLI output context '{}' has no Elasticsearch service",
                        output_context.context
                    ));
                }
            }
            #[cfg(not(feature = "elasticrc"))]
            return Err(eyre!(
                "Configured Elastic CLI output context '{}' requires the 'elasticrc' feature",
                output_context.context
            ));
        } else if let Some(output_name) = &self.output.default {
            let output = hosts
                .get(output_name)
                .ok_or_else(|| eyre!("Configured output host '{output_name}' was not found"))?;
            if !output.has_role(HostRole::Send) {
                return Err(eyre!("Configured output host '{output_name}' must include role 'send'"));
            }
            if output.app() != Some(Application::Elasticsearch) {
                return Err(eyre!(
                    "Configured output host '{output_name}' must be an Elasticsearch host"
                ));
            }

            let viewer_name = output
                .viewer()
                .ok_or_else(|| eyre!("Configured output host '{output_name}' has no Kibana viewer"))?;
            let viewer = hosts.get(viewer_name).ok_or_else(|| {
                eyre!("Configured output host '{output_name}' references unknown viewer host '{viewer_name}'")
            })?;
            if !viewer.has_role(HostRole::View) || viewer.app() != Some(Application::Kibana) {
                return Err(eyre!(
                    "Configured output host '{output_name}' viewer '{viewer_name}' must be a Kibana host with role 'view'"
                ));
            }
        }

        if let Some(job_name) = &self.job.default {
            let jobs = load_saved_jobs()?;
            if !jobs.contains_key(job_name) {
                return Err(eyre!("Configured default job '{job_name}' was not found"));
            }
        }

        Ok(())
    }

    pub fn validate_version(&self) -> Result<()> {
        if self.version != CURRENT_VERSION {
            return Err(eyre!(
                "Unsupported application configuration version {}; this ESDiag version supports version {CURRENT_VERSION}",
                self.version
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ApplicationConfig, JobConfig, OnboardingWorkflow, OutputConfig};
    use crate::data::{Application, ElasticOutputContext, HostRole, KnownHostBuilder};
    use std::collections::BTreeMap;
    use url::Url;

    fn linked_output_hosts() -> BTreeMap<String, crate::data::KnownHost> {
        let output = KnownHostBuilder::new(Url::parse("https://es.example:9200").expect("url"))
            .application(Application::Elasticsearch)
            .roles(vec![HostRole::Send])
            .viewer(Some("output-kibana".to_string()))
            .build()
            .expect("output host");
        let viewer = KnownHostBuilder::new(Url::parse("https://kb.example:5601").expect("url"))
            .application(Application::Kibana)
            .roles(vec![HostRole::View])
            .build()
            .expect("viewer host");
        BTreeMap::from([
            ("output-elasticsearch".to_string(), output),
            ("output-kibana".to_string(), viewer),
        ])
    }

    #[test]
    fn config_round_trips_with_reference_values_only() {
        let env = crate::TestEnv::new();
        let config = ApplicationConfig {
            version: 1,
            user: Some("reno@example.com".to_string()),
            workflow: Some(OnboardingWorkflow::CollectAndProcess),
            output: OutputConfig {
                default: Some("output-elasticsearch".to_string()),
                ..OutputConfig::default()
            },
            job: JobConfig {
                default: Some("production-standard".to_string()),
            },
        };

        config.save().expect("save configuration");
        let written = std::fs::read_to_string(ApplicationConfig::path().expect("config path")).expect("read config");
        let loaded = ApplicationConfig::load().expect("load configuration");

        assert_eq!(loaded, config);
        assert!(written.contains("default: output-elasticsearch"));
        assert!(written.contains("default: production-standard"));
        assert!(written.contains("workflow: collect-and-process"));
        for forbidden in ["apikey", "password", "authorization", "https://es.example"] {
            assert!(!written.contains(forbidden), "{forbidden} must not be serialized");
        }
        assert!(
            !env.settings_path.exists(),
            "application config must not write legacy settings"
        );
    }

    #[test]
    fn elastic_output_context_round_trips_without_credentials() {
        let _env = crate::TestEnv::new();
        let config = ApplicationConfig {
            version: 1,
            output: OutputConfig {
                elastic_context: Some(ElasticOutputContext {
                    context: "monitoring".to_string(),
                    config_file: Some("elasticrc.yml".into()),
                }),
                ..OutputConfig::default()
            },
            ..ApplicationConfig::new()
        };

        config.save().expect("save configuration");
        let written = std::fs::read_to_string(ApplicationConfig::path().expect("config path")).expect("read config");
        let loaded = ApplicationConfig::load().expect("load configuration");

        assert_eq!(loaded, config);
        assert!(written.contains("context: monitoring"));
        assert!(written.contains("config_file: elasticrc.yml"));
        assert!(!written.contains("api_key"));
        assert!(!written.contains("password"));
    }

    #[test]
    fn unknown_version_is_rejected_without_rewriting_configuration() {
        let _env = crate::TestEnv::new();
        let path = ApplicationConfig::path().expect("config path");
        let content = "version: 2\nuser: future@example.com\n";
        std::fs::write(&path, content).expect("write future config");

        let err = ApplicationConfig::load().expect_err("future version must be rejected");

        assert!(
            err.to_string()
                .contains("Unsupported application configuration version 2")
        );
        assert_eq!(std::fs::read_to_string(path).expect("read original config"), content);
    }

    #[test]
    fn configuration_without_a_workflow_remains_readable() {
        let _env = crate::TestEnv::new();
        let path = ApplicationConfig::path().expect("config path");
        std::fs::write(&path, "version: 1\nuser: existing@example.com\n").expect("write old configuration");

        let config = ApplicationConfig::load().expect("load old configuration");

        assert_eq!(config.user.as_deref(), Some("existing@example.com"));
        assert_eq!(config.workflow, None);
    }

    #[test]
    fn reference_validation_requires_a_linked_elasticsearch_output() {
        let _env = crate::TestEnv::new();
        crate::data::KnownHost::write_hosts_yml(&linked_output_hosts()).expect("write hosts");
        let config = ApplicationConfig {
            version: 1,
            output: OutputConfig {
                default: Some("output-elasticsearch".to_string()),
                ..OutputConfig::default()
            },
            ..ApplicationConfig::new()
        };

        config.validate_references().expect("linked output must validate");
    }

    #[test]
    fn reference_validation_rejects_unknown_output() {
        let _env = crate::TestEnv::new();
        let config = ApplicationConfig {
            version: 1,
            output: OutputConfig {
                default: Some("missing".to_string()),
                ..OutputConfig::default()
            },
            ..ApplicationConfig::new()
        };

        let err = config.validate_references().expect_err("unknown host must fail");

        assert!(
            err.to_string()
                .contains("Configured output host 'missing' was not found")
        );
    }

    #[test]
    fn reference_validation_rejects_unknown_default_job() {
        let _env = crate::TestEnv::new();
        let config = ApplicationConfig {
            version: 1,
            job: JobConfig {
                default: Some("missing-job".to_string()),
            },
            ..ApplicationConfig::new()
        };

        let err = config.validate_references().expect_err("unknown default job must fail");

        assert!(
            err.to_string()
                .contains("Configured default job 'missing-job' was not found")
        );
    }
}
