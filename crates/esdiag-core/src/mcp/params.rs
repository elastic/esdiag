// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Tool parameters. Names match the CLI options. No struct accepts a raw
//! credential or the process-wide `sources` override, and unknown fields are
//! rejected so a client cannot pass either. The one exception is the
//! service-mode download token in [`ServiceProcessParams`].

use rmcp::schemars::JsonSchema;
use serde::Deserialize;

fn default_diagnostic_type() -> String {
    "standard".to_string()
}

fn default_upload_api_url() -> String {
    crate::elastic_upload_service::DEFAULT_UPLOAD_API_URL.to_string()
}

#[cfg(feature = "agent")]
fn default_agent() -> String {
    crate::operations::agent::DEFAULT_AGENT_BUILDER_AGENT.to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct HostAuthParams {
    /// Saved host name or resolved template reference to test
    pub target: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct NameParams {
    /// Name of the saved entry
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct ProcessParams {
    /// Source to read diagnostic data from: archive, directory, known host, or Elastic Upload Service URL
    pub input: String,
    /// Target for processed documents: a saved Elasticsearch host, file, or directory; defaults to the configured output. `-` (stdout) is rejected.
    pub output: Option<String>,
    /// Ask Agent Builder about the diagnostic after it is processed
    #[cfg(feature = "agent")]
    pub ask: Option<String>,
    /// Diagnostic report account name
    pub account: Option<String>,
    /// Diagnostic report case number
    pub case: Option<String>,
    /// Diagnostic report opportunity
    pub opportunity: Option<String>,
    /// Diagnostic report user
    pub user: Option<String>,
    /// Force scrubbed archive mode on or off; omitted detects it from the file name
    pub scrubbed: Option<bool>,
    /// Save the effective invocation as a named job before processing
    #[cfg(feature = "keystore")]
    pub save_job: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct ServiceProcessParams {
    /// Elastic Upload Service download URL: `https://upload.elastic.co/...` without an embedded token
    pub url: String,
    /// Elastic Upload Service download token for `url`
    pub token: String,
}

impl std::fmt::Debug for ServiceProcessParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceProcessParams")
            .field("url", &self.url)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct CollectParams {
    /// Saved known host with role `collect`, or an Elastic CLI context reference
    pub host: String,
    /// Existing local directory that receives the diagnostic archive
    pub output: String,
    /// Diagnostic type: minimal, light, standard, or support
    #[serde(rename = "type", default = "default_diagnostic_type")]
    #[schemars(rename = "type")]
    pub diagnostic_type: String,
    /// APIs to include
    pub include: Option<Vec<String>>,
    /// APIs to exclude
    pub exclude: Option<Vec<String>>,
    /// Diagnostic report account name
    pub account: Option<String>,
    /// Diagnostic report case number
    pub case: Option<String>,
    /// Diagnostic report opportunity
    pub opportunity: Option<String>,
    /// Diagnostic report user
    pub user: Option<String>,
    /// Elastic Upload Service ID or URL to send the archive to after collection
    pub send: Option<String>,
    /// Save the effective invocation as a named job before collecting
    #[cfg(feature = "keystore")]
    pub save_job: Option<String>,
}

#[cfg(feature = "agent")]
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct AgentAskParams {
    /// Prompt passed unchanged to Agent Builder
    pub prompt: String,
    /// Agent Builder agent identifier
    #[serde(default = "default_agent")]
    pub agent: String,
    /// Continue this Kibana Agent Builder conversation; omitted starts a new one
    pub conversation: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct SendParams {
    /// Local diagnostic archive file path
    pub file_name: String,
    /// Elastic Upload Service ID or URL
    pub upload_id: String,
    /// Elastic Upload Service base URL
    #[serde(default = "default_upload_api_url")]
    pub api_url: String,
}

#[cfg(feature = "setup")]
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct SetupOutputParams {
    /// Saved output host with role `send`; defaults to the persisted output
    pub name: Option<String>,
    /// Start an Elasticsearch trial license when Agent Builder is not licensed
    #[serde(default)]
    pub activate_trial: bool,
}

#[cfg(feature = "setup")]
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct SetupHostParams {
    /// Saved host; defaults to the persisted output
    pub host: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct HostAddParams {
    /// A name to identify this host
    pub name: String,
    /// A concrete URL, a template definition target, or a resolved template reference
    pub target: String,
    /// Application of this host when the target cannot infer it: elasticsearch, kibana, or logstash
    pub app: Option<String>,
    /// Persist `target` as a saved host URL template
    #[serde(default)]
    pub url_template: bool,
    /// Accept invalid certificates
    pub accept_invalid_certs: Option<bool>,
    /// Existing keystore secret that authenticates this host
    pub secret: Option<String>,
    /// Host roles: collect, send, view
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct HostUpdateParams {
    /// Name of the saved host to update
    pub name: String,
    /// Accept invalid certificates
    pub accept_invalid_certs: Option<bool>,
    /// Existing keystore secret that authenticates this host
    pub secret: Option<String>,
    /// Host roles: collect, send, view
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(deny_unknown_fields)]
pub struct OutputSetParams {
    /// Elastic CLI context name
    pub context: String,
}
