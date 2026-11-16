/*
 * Licensed to Elasticsearch B.V. under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch B.V. licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *	http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

//! Read-only discovery, parsing, and lazy resolution of Elastic CLI contexts.
//!
//! Loading a [`ConfigFile`] validates only its top-level shape and never executes
//! resolver expressions. Call [`ServiceConfig::resolve`] on one selected service
//! configuration to produce a runtime [`Service`].
//!
//! Runtime credentials use [`SecretString`] so common debug formatting does not
//! reveal secret values. Callers should keep those values runtime-only.
//!
//! ```
//! use elasticrc::ConfigFile;
//!
//! # fn example() -> Result<(), elasticrc::Error> {
//! let config = ConfigFile::load_with_options(None, None)?;
//! let elasticsearch = config
//!     .current()?
//!     .elasticsearch
//!     .as_ref()
//!     .expect("current context has no Elasticsearch service")
//!     .resolve()?;
//! println!("Elasticsearch URL: {}", elasticsearch.url);
//! # Ok(())
//! # }
//! ```

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows", test))]
use keyring_core::api::CredentialStoreApi;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    env,
    fmt::Display,
    fs,
    io::{self, Read},
    marker::PhantomData,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use url::Url;

/// A string whose `Debug` representation redacts its contents.
pub type SecretString = redact::Secret<String>;
const FILE_RESOLVER_MAX_BYTES: u64 = 1024 * 1024;
const COMMAND_RESOLVER_TIMEOUT: Duration = Duration::from_secs(5);
const ELASTIC_CREDENTIAL_ENV_VARS: [&str; 9] = [
    "ELASTIC_ES_API_KEY",
    "ELASTIC_ES_USERNAME",
    "ELASTIC_ES_PASSWORD",
    "ELASTIC_KIBANA_API_KEY",
    "ELASTIC_KIBANA_USERNAME",
    "ELASTIC_KIBANA_PASSWORD",
    "ELASTIC_CLOUD_API_KEY",
    "ELASTIC_CLOUD_USERNAME",
    "ELASTIC_CLOUD_PASSWORD",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
/// Raw Elastic CLI configuration.
///
/// Secret fields may contain inline values or unresolved expressions. Resolver
/// expressions are evaluated only when a service is explicitly resolved.
pub struct ConfigFile {
    current_context: String,
    contexts: BTreeMap<String, Context>,
}

impl<'de> Deserialize<'de> for ConfigFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ConfigFileData {
            current_context: String,
            contexts: BTreeMap<String, Context>,
        }

        let data = ConfigFileData::deserialize(deserializer)?;
        let mut config = Self {
            current_context: data.current_context,
            contexts: data.contexts,
        };
        config.attach_context_names();
        Ok(config)
    }
}

impl ConfigFile {
    fn attach_context_names(&mut self) {
        for (context_name, context) in &mut self.contexts {
            context.attach_name(context_name);
        }
    }

    /// Load a JSON or YAML Elastic CLI config from an explicit path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        load_config_file(path)
    }

    /// Discover and load the first readable config in an explicit home directory.
    pub fn load_from_home(home_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let path = discover_config_path(home_dir.as_ref()).ok_or_else(|| Error::ConfigNotFound {
            home_dir: home_dir.as_ref().to_path_buf(),
        })?;
        Self::load(path)
    }

    /// Load from an explicit path, `ELASTIC_CLI_CONFIG_FILE`, or home discovery.
    pub fn load_with_options(explicit_path: Option<&Path>, home_dir: Option<&Path>) -> Result<Self, Error> {
        if let Some(path) = explicit_path {
            return Self::load(path);
        }
        if let Some(path) = env::var_os("ELASTIC_CLI_CONFIG_FILE") {
            return Self::load(PathBuf::from(path));
        }
        let home_dir = home_dir
            .map(Path::to_path_buf)
            .or_else(home_dir_from_env)
            .ok_or(Error::HomeDirectoryUnavailable)?;
        Self::load_from_home(home_dir)
    }

    /// Validate all raw service blocks without evaluating resolver expressions.
    pub fn validate(&self) -> Result<(), Error> {
        self.validate_shape()?;
        for context in self.contexts.values() {
            context.validate()?;
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), Error> {
        if self.current_context.trim().is_empty() {
            return Err(Error::InvalidShape("current_context must not be empty".to_string()));
        }
        if self.contexts.is_empty() {
            return Err(Error::InvalidShape("contexts must not be empty".to_string()));
        }
        Ok(())
    }

    /// Return a named context.
    pub fn context(&self, name: &str) -> Result<&Context, Error> {
        self.contexts.get(name).ok_or_else(|| Error::MissingContext {
            name: name.to_string(),
            available: self.contexts.keys().cloned().collect(),
        })
    }

    /// Return the configured current context name.
    pub fn current_context_name(&self) -> &str {
        &self.current_context
    }

    /// Return all configured contexts.
    pub fn contexts(&self) -> &BTreeMap<String, Context> {
        &self.contexts
    }

    /// Return the current context.
    pub fn current(&self) -> Result<&Context, Error> {
        self.contexts
            .get(&self.current_context)
            .ok_or_else(|| Error::MissingContext {
                name: self.current_context.clone(),
                available: self.contexts.keys().cloned().collect(),
            })
    }
}

/// Marker for an Elasticsearch service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Elasticsearch;

/// Marker for a Kibana service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kibana;

/// Marker for an Elastic Cloud service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cloud;

mod private {
    pub trait Sealed {}

    impl Sealed for super::Elasticsearch {}
    impl Sealed for super::Kibana {}
    impl Sealed for super::Cloud {}
}

/// A service type supported by the Elastic CLI configuration schema.
pub trait ServiceType: private::Sealed {
    /// Canonical name used by the Elastic CLI configuration schema.
    const NAME: &'static str;
}

impl ServiceType for Elasticsearch {
    const NAME: &'static str = "elasticsearch";
}

impl ServiceType for Kibana {
    const NAME: &'static str = "kibana";
}

impl ServiceType for Cloud {
    const NAME: &'static str = "cloud";
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Services configured for one Elastic CLI context.
pub struct Context {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elasticsearch: Option<ServiceConfig<Elasticsearch>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kibana: Option<ServiceConfig<Kibana>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<ServiceConfig<Cloud>>,
}

impl Context {
    fn attach_name(&mut self, context_name: &str) {
        if let Some(service) = &mut self.elasticsearch {
            service.context = Some(context_name.to_string());
        }
        if let Some(service) = &mut self.kibana {
            service.context = Some(context_name.to_string());
        }
        if let Some(service) = &mut self.cloud {
            service.context = Some(context_name.to_string());
        }
    }

    fn validate(&self) -> Result<(), Error> {
        if let Some(service) = &self.elasticsearch {
            service.validate()?;
        }
        if let Some(service) = &self.kibana {
            service.validate()?;
        }
        if let Some(service) = &self.cloud {
            service.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Raw URL and optional authentication for one context service.
#[serde(bound = "")]
pub struct ServiceConfig<T> {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(skip)]
    context: Option<String>,
    #[serde(skip)]
    service_type: PhantomData<T>,
}

impl<T: ServiceType> ServiceConfig<T> {
    /// Construct a raw service configuration.
    pub fn new(url: impl Into<String>, auth: Option<AuthConfig>) -> Self {
        Self {
            url: url.into(),
            auth,
            context: None,
            service_type: PhantomData,
        }
    }

    fn validate(&self) -> Result<(), Error> {
        let url = Url::parse(&self.url).map_err(|source| Error::InvalidServiceUrl {
            context: self.context.clone(),
            service: T::NAME,
            value: redact_url(&self.url),
            source,
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::InvalidServiceUrlScheme {
                context: self.context.clone(),
                service: T::NAME,
                value: redact_url(&self.url),
            });
        }
        if let Some(auth) = &self.auth {
            auth.validate(self.context.as_deref(), T::NAME)?;
        }
        Ok(())
    }

    /// Resolve expressions and produce a validated runtime service.
    pub fn resolve(&self) -> Result<Service<T>, Error> {
        let field = |name: &str| match &self.context {
            Some(context) => format!("{context}.{}.{name}", T::NAME),
            None => format!("{}.{name}", T::NAME),
        };
        let url_value = resolve_string_expressions(&self.url, &field("url"))?;
        let url = Url::parse(&url_value).map_err(|source| Error::InvalidServiceUrl {
            context: self.context.clone(),
            service: T::NAME,
            value: redact_url(&self.url),
            source,
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::InvalidServiceUrlScheme {
                context: self.context.clone(),
                service: T::NAME,
                value: redact_url(&self.url),
            });
        }
        let auth = if let Some(auth) = &self.auth {
            auth.validate(self.context.as_deref(), T::NAME)?;
            auth.resolve(self.context.as_deref(), T::NAME)?
        } else {
            Auth::None
        };

        Ok(Service {
            url,
            auth,
            service_type: PhantomData,
        })
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
/// Raw authentication configuration.
///
/// Debug output redacts API keys and passwords, including inline values.
pub struct AuthConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl AuthConfig {
    fn validate(&self, context_name: Option<&str>, service: &'static str) -> Result<(), Error> {
        match (&self.api_key, &self.username, &self.password) {
            (Some(_), None, None) | (None, None, None) | (None, Some(_), Some(_)) => Ok(()),
            (Some(_), _, _) => Err(Error::InvalidAuth {
                context: context_name.map(str::to_string),
                service,
                message: "api_key cannot be combined with username or password".to_string(),
            }),
            (None, Some(_), None) | (None, None, Some(_)) => Err(Error::InvalidAuth {
                context: context_name.map(str::to_string),
                service,
                message: "basic authentication requires both username and password".to_string(),
            }),
        }
    }

    fn resolve(&self, context_name: Option<&str>, service: &'static str) -> Result<Auth, Error> {
        let resolve = |value: &str, name: &str| {
            let field = match context_name {
                Some(context) => format!("{context}.{service}.auth.{name}"),
                None => format!("{service}.auth.{name}"),
            };
            resolve_string_expressions(value, &field)
        };
        match (&self.api_key, &self.username, &self.password) {
            (Some(api_key), None, None) => Ok(Auth::api_key(resolve(api_key, "api_key")?)),
            (None, Some(username), Some(password)) => Ok(Auth::basic(
                resolve(username, "username")?,
                resolve(password, "password")?,
            )),
            (None, None, None) => Ok(Auth::None),
            _ => Err(Error::InvalidShape("invalid auth block".to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Parsed `.service` or `.context.service` reference.
pub enum ContextServiceReference {
    Elasticsearch { context: Option<String> },
    Kibana { context: Option<String> },
    Cloud { context: Option<String> },
}

impl ContextServiceReference {
    /// Parse a leading-dot service reference.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.strip_prefix('.')?;
        if value.is_empty() || value.contains('/') || value.contains('\\') {
            return None;
        }
        let (context, service) = match value.rsplit_once('.') {
            Some((context, service)) => {
                if context.is_empty() {
                    return None;
                }
                (Some(context.to_string()), service)
            }
            None => (None, value),
        };
        match service {
            "elasticsearch" | "es" => Some(Self::Elasticsearch { context }),
            "kibana" | "kb" => Some(Self::Kibana { context }),
            "cloud" => Some(Self::Cloud { context }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A runtime service with a validated URL and resolved authentication.
pub struct Service<T> {
    pub url: Url,
    pub auth: Auth,
    service_type: PhantomData<T>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Runtime authentication for a selected service.
pub enum Auth {
    ApiKey(SecretString),
    Basic {
        username: String,
        password: SecretString,
    },
    #[default]
    None,
}

impl Auth {
    /// Construct redacted API key authentication.
    pub fn api_key(api_key: impl Into<String>) -> Self {
        Self::ApiKey(SecretString::new(api_key.into()))
    }

    /// Construct basic authentication with a redacted password.
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: SecretString::new(password.into()),
        }
    }
}

#[derive(Debug)]
/// Errors produced while discovering, parsing, validating, or resolving config.
pub enum Error {
    ConfigNotFound {
        home_dir: PathBuf,
    },
    ExecutableConfigUnsupported {
        path: PathBuf,
    },
    HomeDirectoryUnavailable,
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    Yaml {
        path: PathBuf,
        source: yaml_serde::Error,
    },
    InvalidShape(String),
    MissingContext {
        name: String,
        available: Vec<String>,
    },
    MissingService {
        context: String,
        service: &'static str,
    },
    InvalidServiceUrl {
        context: Option<String>,
        service: &'static str,
        /// The configured URL before resolution, with any userinfo redacted.
        value: String,
        source: url::ParseError,
    },
    InvalidServiceUrlScheme {
        context: Option<String>,
        service: &'static str,
        /// The configured URL before resolution, with any userinfo redacted.
        value: String,
    },
    InvalidAuth {
        context: Option<String>,
        service: &'static str,
        message: String,
    },
    InvalidResolverExpression {
        field: String,
        /// What is wrong with the expression; never the field's value, which may be a secret.
        value: String,
    },
    UnknownResolver {
        resolver: String,
        field: String,
    },
    ShellSyntaxUnsupported {
        resolver: String,
        field: String,
    },
    ResolverFailed {
        resolver: String,
        field: String,
        message: String,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigNotFound { home_dir } => {
                write!(f, "no Elastic CLI config file found in {}", home_dir.display())
            }
            Self::ExecutableConfigUnsupported { path } => {
                write!(
                    f,
                    "executable Elastic CLI config format is not supported: {}",
                    path.display()
                )
            }
            Self::HomeDirectoryUnavailable => write!(f, "home directory is unavailable"),
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "failed to parse JSON config {}: {source}", path.display()),
            Self::Yaml { path, source } => write!(f, "failed to parse YAML config {}: {source}", path.display()),
            Self::InvalidShape(message) => write!(f, "invalid Elastic CLI config shape: {message}"),
            Self::MissingContext { name, available } => {
                write!(f, "Elastic CLI context '{name}' was not found")?;
                if !available.is_empty() {
                    write!(f, "; available contexts: {}", available.join(", "))?;
                }
                Ok(())
            }
            Self::MissingService { context, service } => {
                write!(f, "Elastic CLI context '{context}' does not define service '{service}'")
            }
            Self::InvalidServiceUrl {
                context,
                service,
                value,
                source,
            } => {
                if let Some(context) = context {
                    write!(
                        f,
                        "invalid URL for Elastic CLI context '{context}' service '{service}' ({value}): {source}"
                    )
                } else {
                    write!(f, "invalid URL for Elastic CLI service '{service}' ({value}): {source}")
                }
            }
            Self::InvalidServiceUrlScheme {
                context,
                service,
                value,
            } => {
                if let Some(context) = context {
                    write!(
                        f,
                        "invalid URL scheme for Elastic CLI context '{context}' service '{service}' ({value}); expected http or https"
                    )
                } else {
                    write!(
                        f,
                        "invalid URL scheme for Elastic CLI service '{service}' ({value}); expected http or https"
                    )
                }
            }
            Self::InvalidAuth {
                context,
                service,
                message,
            } => {
                if let Some(context) = context {
                    write!(
                        f,
                        "invalid auth for Elastic CLI context '{context}' service '{service}': {message}"
                    )
                } else {
                    write!(f, "invalid auth for Elastic CLI service '{service}': {message}")
                }
            }
            Self::InvalidResolverExpression { field, value } => {
                write!(f, "invalid resolver expression in field '{field}': {value}")
            }
            Self::UnknownResolver { resolver, field } => {
                write!(f, "unknown resolver '{resolver}' in field '{field}'")
            }
            Self::ShellSyntaxUnsupported { resolver, field } => write!(
                f,
                "resolver '{resolver}' in field '{field}' requires shell interpretation, which is unsupported"
            ),
            Self::ResolverFailed {
                resolver,
                field,
                message,
            } => write!(f, "resolver '{resolver}' failed for field '{field}': {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Find the first readable Elastic CLI config in its documented discovery order.
pub fn discover_config_path(home_dir: &Path) -> Option<PathBuf> {
    [".elasticrc", ".elasticrc.json", ".elasticrc.yaml", ".elasticrc.yml"]
        .into_iter()
        .map(|name| home_dir.join(name))
        .find(|path| path.is_file() && fs::File::open(path).is_ok())
}

fn load_config_file(path: impl AsRef<Path>) -> Result<ConfigFile, Error> {
    let path = path.as_ref();
    reject_executable_config(path)?;
    let contents = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let config: ConfigFile = if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(&contents).map_err(|source| Error::Json {
            path: path.to_path_buf(),
            source,
        })?
    } else {
        yaml_serde::from_str(&contents).map_err(|source| Error::Yaml {
            path: path.to_path_buf(),
            source,
        })?
    };
    if let Some(warning) = inline_secret_permission_warning(path, &config) {
        tracing::warn!("{warning}");
    }
    config.validate_shape()?;
    Ok(config)
}

fn reject_executable_config(path: &Path) -> Result<(), Error> {
    if matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("js" | "ts" | "mjs" | "cjs")
    ) {
        return Err(Error::ExecutableConfigUnsupported {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn home_dir_from_env() -> Option<PathBuf> {
    match std::env::consts::OS {
        "windows" => env::var_os("USERPROFILE").map(PathBuf::from),
        "linux" | "macos" => env::var_os("HOME").map(PathBuf::from),
        _ => None,
    }
}

/// Return a warning when inline secrets are stored with loose Unix permissions.
///
/// Returns `None` on platforms without Unix permission bits.
pub fn inline_secret_permission_warning(path: &Path, config: &ConfigFile) -> Option<String> {
    if !config.contains_inline_secret() {
        return None;
    }
    loose_permissions(path).then(|| {
        format!(
            "Elastic CLI config {} contains inline secrets and has permissions broader than 0600/0400.",
            path.display()
        )
    })
}

impl ConfigFile {
    fn contains_inline_secret(&self) -> bool {
        self.contexts.values().any(Context::contains_inline_secret)
    }
}

impl Context {
    fn contains_inline_secret(&self) -> bool {
        self.elasticsearch
            .as_ref()
            .is_some_and(ServiceConfig::contains_inline_secret)
            || self.kibana.as_ref().is_some_and(ServiceConfig::contains_inline_secret)
            || self.cloud.as_ref().is_some_and(ServiceConfig::contains_inline_secret)
    }
}

impl<T> ServiceConfig<T> {
    fn contains_inline_secret(&self) -> bool {
        self.auth.as_ref().is_some_and(AuthConfig::contains_inline_secret)
    }
}

impl AuthConfig {
    fn contains_inline_secret(&self) -> bool {
        [&self.api_key, &self.password]
            .into_iter()
            .flatten()
            .any(|value| !is_resolver_expression(value))
    }
}

/// Replace URL userinfo with `***` so credentials embedded in a configured URL stay out of errors.
fn redact_url(value: &str) -> String {
    let Some(scheme_end) = value.find("://") else {
        return value.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = value[authority_start..]
        .find(['/', '?', '#'])
        .map_or(value.len(), |offset| authority_start + offset);
    match value[authority_start..authority_end].rfind('@') {
        Some(at) => format!("{}***{}", &value[..authority_start], &value[authority_start + at..]),
        None => value.to_string(),
    }
}

fn is_resolver_expression(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("$(") && value.ends_with(')')
}

#[cfg(unix)]
fn loose_permissions(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o177 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn loose_permissions(_path: &Path) -> bool {
    false
}

fn resolve_string_expressions(value: &str, field: &str) -> Result<String, Error> {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("$(") {
        output.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find(')') else {
            return Err(Error::InvalidResolverExpression {
                field: field.to_string(),
                value: "unterminated `$(`".to_string(),
            });
        };
        let expression = &after_start[..end];
        output.push_str(&resolve_expression(expression, field)?);
        remaining = &after_start[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn resolve_expression(expression: &str, field: &str) -> Result<String, Error> {
    let (resolver, params) = expression
        .split_once(':')
        .ok_or_else(|| Error::InvalidResolverExpression {
            field: field.to_string(),
            value: "expected `$(resolver:parameters)`".to_string(),
        })?;
    match resolver {
        "env" => env::var(params).map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        }),
        "file" => resolve_file_expression(params, resolver, field),
        "cmd" => {
            tracing::warn!("Elastic CLI config is executing a command-backed resolver for {field}");
            resolve_command_expression(params, resolver, field).map(|output| output.trim().to_string())
        }
        "pass" => {
            tracing::warn!("Elastic CLI config is executing a command-backed resolver for {field}");
            resolve_pass_expression(params, resolver, field)
        }
        "keychain" => resolve_keyring_expression(params, resolver, field),
        "secret_service" => resolve_keyring_expression(params, resolver, field),
        "credential_manager" => resolve_keyring_expression(params, resolver, field),
        _ => Err(Error::UnknownResolver {
            resolver: resolver.to_string(),
            field: field.to_string(),
        }),
    }
}

fn resolve_file_expression(path: &str, resolver: &str, field: &str) -> Result<String, Error> {
    let path = Path::new(path);
    let metadata = fs::metadata(path).map_err(|source| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: source.to_string(),
    })?;
    if !metadata.is_file() {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: format!("{} is not a regular file", path.display()),
        });
    }
    if metadata.len() > FILE_RESOLVER_MAX_BYTES {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: format!("{} exceeds resolver size limit", path.display()),
        });
    }
    let file = fs::File::open(path).map_err(|source| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: source.to_string(),
    })?;
    read_file_resolver_value(file, path, resolver, field)
}

fn read_file_resolver_value(reader: impl Read, path: &Path, resolver: &str, field: &str) -> Result<String, Error> {
    let mut contents = Vec::new();
    let bytes_read = reader
        .take(FILE_RESOLVER_MAX_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        })?;
    if bytes_read as u64 > FILE_RESOLVER_MAX_BYTES {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: format!("{} exceeds resolver size limit", path.display()),
        });
    }
    String::from_utf8(contents)
        .map(|value| value.trim().to_string())
        .map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        })
}

fn resolve_pass_expression(path: &str, resolver: &str, field: &str) -> Result<String, Error> {
    let args = vec!["show".to_string(), path.to_string()];
    let output = run_command("pass", &args, resolver, field)?;
    Ok(output.lines().next().unwrap_or_default().trim().to_string())
}

fn resolve_command_expression(command: &str, resolver: &str, field: &str) -> Result<String, Error> {
    let argv = parse_command_argv(command, resolver, field)?;
    let (program, args) = argv.split_first().ok_or_else(|| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: "command resolver is empty".to_string(),
    })?;
    run_command(program, args, resolver, field)
}

fn parse_command_argv(command: &str, resolver: &str, field: &str) -> Result<Vec<String>, Error> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut token_started = false;

    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(quote_char) if ch == quote_char => {
                quote = None;
            }
            Some(_) if ch == '\\' => {
                if let Some(next) = chars.next_if(|next| next.is_whitespace() || matches!(next, '\'' | '"' | '\\')) {
                    current.push(next);
                } else {
                    current.push(ch);
                }
                token_started = true;
            }
            Some(_) => {
                current.push(ch);
                token_started = true;
            }
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                token_started = true;
            }
            None if ch == '\\' => {
                if let Some(next) = chars.next_if(|next| next.is_whitespace() || matches!(next, '\'' | '"' | '\\')) {
                    current.push(next);
                } else {
                    current.push(ch);
                }
                token_started = true;
            }
            None if matches!(
                ch,
                '|' | '&' | ';' | '<' | '>' | '(' | ')' | '{' | '}' | '`' | '\n' | '\r'
            ) =>
            {
                return Err(Error::ShellSyntaxUnsupported {
                    resolver: resolver.to_string(),
                    field: field.to_string(),
                });
            }
            None if ch.is_whitespace() => {
                if token_started {
                    argv.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            None => {
                current.push(ch);
                token_started = true;
            }
        }
    }

    if quote.is_some() {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: "command resolver contains an unterminated quote".to_string(),
        });
    }
    if token_started {
        argv.push(current);
    }
    Ok(argv)
}

fn read_command_output(mut reader: impl Read, stream: &str) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    let max_bytes = FILE_RESOLVER_MAX_BYTES as usize;

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(bytes_read) > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("command {stream} exceeded {max_bytes} bytes"),
            ));
        }
        output.extend_from_slice(&buffer[..bytes_read]);
    }
}

fn run_command(program: &str, args: &[String], resolver: &str, field: &str) -> Result<String, Error> {
    run_command_with_timeout(program, args, resolver, field, COMMAND_RESOLVER_TIMEOUT)
}

fn run_command_with_timeout(
    program: &str,
    args: &[String],
    resolver: &str,
    field: &str,
    timeout: Duration,
) -> Result<String, Error> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in ELASTIC_CREDENTIAL_ENV_VARS {
        command.env_remove(name);
    }
    let mut child = command.spawn().map_err(|source| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: source.to_string(),
    })?;
    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");
    let stdout_reader = thread::spawn(move || read_command_output(stdout, "stdout"));
    let stderr_reader = thread::spawn(move || read_command_output(stderr, "stderr"));

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(Error::ResolverFailed {
                    resolver: resolver.to_string(),
                    field: field.to_string(),
                    message: source.to_string(),
                });
            }
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(Error::ResolverFailed {
                resolver: resolver.to_string(),
                field: field.to_string(),
                message: "command timed out".to_string(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    };

    let stdout_result = stdout_reader.join();
    let stderr_result = stderr_reader.join();
    let stdout = stdout_result
        .map_err(|_| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: "stdout reader panicked".to_string(),
        })?
        .map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        })?;
    let _stderr = stderr_result
        .map_err(|_| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: "stderr reader panicked".to_string(),
        })?
        .map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        })?;

    if !status.success() {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: format!(
                "command exited with {status}; stderr omitted because resolver output may contain secrets"
            ),
        });
    }
    String::from_utf8(stdout).map_err(|source| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: source.to_string(),
    })
}

fn parse_keyring_params(params: &str, resolver: &str, field: &str) -> Result<(String, String), Error> {
    let (service, account) = params.split_once('/').ok_or_else(|| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message: "expected service/account".to_string(),
    })?;
    if service.is_empty() || account.is_empty() {
        return Err(Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: "expected non-empty service/account".to_string(),
        });
    }
    Ok((service.to_string(), account.to_string()))
}

fn resolve_keyring_expression(params: &str, resolver: &str, field: &str) -> Result<String, Error> {
    let (service, account) = parse_keyring_params(params, resolver, field)?;
    resolve_platform_keyring_secret(resolver, &service, &account).map_err(|message| Error::ResolverFailed {
        resolver: resolver.to_string(),
        field: field.to_string(),
        message,
    })
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows", test))]
fn read_keyring_store(
    store: &(impl CredentialStoreApi + ?Sized),
    service: &str,
    account: &str,
) -> Result<String, String> {
    store
        .build(service, account, None)
        .and_then(|entry| entry.get_password())
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "keychain" {
        return Err(format!("resolver '{resolver}' is not supported on macOS"));
    }
    use apple_native_keyring_store::keychain;
    let store = keychain::Store::new().map_err(|err| err.to_string())?;
    read_keyring_store(store.as_ref(), service, account)
}

#[cfg(target_os = "linux")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "secret_service" {
        return Err(format!("resolver '{resolver}' is not supported on Linux"));
    }
    let store = zbus_secret_service_keyring_store::Store::new().map_err(|err| err.to_string())?;
    read_keyring_store(store.as_ref(), service, account)
}

#[cfg(target_os = "windows")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "credential_manager" {
        return Err(format!("resolver '{resolver}' is not supported on Windows"));
    }
    let store = windows_native_keyring_store::Store::new().map_err(|err| err.to_string())?;
    read_keyring_store(store.as_ref(), service, account)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn resolve_platform_keyring_secret(resolver: &str, _service: &str, _account: &str) -> Result<String, String> {
    Err(format!("resolver '{resolver}' is not supported on this platform"))
}

#[cfg(test)]
mod tests {
    use super::{
        Auth, ConfigFile, ContextServiceReference, ELASTIC_CREDENTIAL_ENV_VARS, Error, FILE_RESOLVER_MAX_BYTES,
        discover_config_path, inline_secret_permission_warning, parse_command_argv, read_command_output,
        read_file_resolver_value, read_keyring_store,
    };
    #[cfg(unix)]
    use super::{run_command, run_command_with_timeout};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::{
        fs,
        io::ErrorKind,
        path::Path,
        sync::{Mutex, OnceLock},
        time::{Duration, Instant},
    };
    use tempfile::TempDir;

    fn write(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write config");
    }

    fn current_elasticsearch(config: &ConfigFile) -> &super::ServiceConfig<super::Elasticsearch> {
        config
            .current()
            .expect("current context")
            .elasticsearch
            .as_ref()
            .expect("Elasticsearch config")
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn context_service_reference_parses_active_context_service() {
        assert_eq!(
            ContextServiceReference::parse(".es"),
            Some(ContextServiceReference::Elasticsearch { context: None })
        );
    }

    #[test]
    fn context_service_reference_parses_named_context_from_rightmost_segment() {
        assert_eq!(
            ContextServiceReference::parse(".prod.us-west.es"),
            Some(ContextServiceReference::Elasticsearch {
                context: Some("prod.us-west".to_string()),
            })
        );
    }

    #[test]
    fn context_service_reference_parses_all_service_keys_and_aliases() {
        assert_eq!(
            ContextServiceReference::parse(".elasticsearch"),
            Some(ContextServiceReference::Elasticsearch { context: None })
        );
        assert_eq!(
            ContextServiceReference::parse(".kb"),
            Some(ContextServiceReference::Kibana { context: None })
        );
        assert_eq!(
            ContextServiceReference::parse(".cloud"),
            Some(ContextServiceReference::Cloud { context: None })
        );
    }

    #[test]
    fn context_service_reference_ignores_unknown_service_segments() {
        assert_eq!(ContextServiceReference::parse(".prod.ls"), None);
        assert_eq!(ContextServiceReference::parse(".unknown"), None);
        assert_eq!(ContextServiceReference::parse("./.es"), None);
    }

    #[test]
    fn auth_debug_redacts_api_key() {
        let auth = Auth::api_key("super-secret");
        let rendered = format!("{auth:?}");

        assert!(rendered.contains("[REDACTED"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn auth_debug_redacts_basic_password() {
        let auth = Auth::basic("elastic", "super-secret");
        let rendered = format!("{auth:?}");

        assert!(rendered.contains("elastic"));
        assert!(rendered.contains("[REDACTED"));
        assert!(!rendered.contains("super-secret"));
    }

    #[cfg(unix)]
    #[test]
    fn command_resolver_child_omits_elastic_credentials() {
        let _guard = env_lock().lock().expect("env lock");
        for name in ELASTIC_CREDENTIAL_ENV_VARS {
            unsafe {
                std::env::set_var(name, "must-not-leak");
            }
        }

        let output = run_command("env", &[], "cmd", "test").expect("run env");

        for name in ELASTIC_CREDENTIAL_ENV_VARS {
            assert!(!output.contains(name), "{name} leaked to resolver child");
            unsafe {
                std::env::remove_var(name);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn command_resolver_kills_timed_out_process() {
        let start = Instant::now();

        let error = run_command_with_timeout(
            "sleep",
            &["30".to_string()],
            "cmd",
            "contexts.prod.elasticsearch.auth.api_key",
            Duration::from_millis(50),
        )
        .expect_err("sleep must time out");

        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(error.to_string().contains("command timed out"));
    }

    #[test]
    fn keyring_reader_returns_secret_without_mutating_global_store() {
        let store = keyring_core::mock::Store::new().expect("mock store");
        let entry = keyring_core::api::CredentialStoreApi::build(store.as_ref(), "elastic-cli", "production", None)
            .expect("mock entry");
        entry.set_password("keyring-secret").expect("set secret");

        let secret = read_keyring_store(store.as_ref(), "elastic-cli", "production").expect("read secret");

        assert_eq!(secret, "keyring-secret");
        assert!(keyring_core::get_default_store().is_none());
    }

    #[test]
    fn raw_auth_debug_redacts_inline_secrets() {
        let auth = super::AuthConfig {
            api_key: Some("inline-api-key".to_string()),
            username: Some("elastic".to_string()),
            password: Some("inline-password".to_string()),
        };

        let rendered = format!("{auth:?}");

        assert!(rendered.contains("elastic"));
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("inline-api-key"));
        assert!(!rendered.contains("inline-password"));
    }

    #[test]
    fn discovers_default_config_in_elastic_cli_order() {
        let tmp = TempDir::new().expect("temp dir");
        write(
            &tmp.path().join(".elasticrc.yml"),
            "current_context: later\ncontexts: {}\n",
        );
        write(&tmp.path().join(".elasticrc"), "current_context: first\ncontexts: {}\n");

        assert_eq!(discover_config_path(tmp.path()), Some(tmp.path().join(".elasticrc")));
    }

    #[cfg(unix)]
    #[test]
    fn discovery_skips_unreadable_config_candidate() {
        let tmp = TempDir::new().expect("temp dir");
        let unreadable = tmp.path().join(".elasticrc");
        let readable = tmp.path().join(".elasticrc.yml");
        write(&unreadable, "current_context: first\ncontexts: {}\n");
        write(&readable, "current_context: later\ncontexts: {}\n");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).expect("set permissions");

        assert_eq!(discover_config_path(tmp.path()), Some(readable));
    }

    #[test]
    fn loads_yaml_config_and_resolves_service() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            r#"
current_context: prod
contexts:
  prod:
    elasticsearch:
      url: https://es.example:9200
      auth:
        api_key: es-key
    kibana:
      url: https://kb.example:5601
"#,
        );

        let config = ConfigFile::load(&path).expect("load config");
        let service = config
            .context("prod")
            .expect("prod context")
            .elasticsearch
            .as_ref()
            .expect("Elasticsearch config")
            .resolve()
            .expect("resolve service");

        assert_eq!(service.url.as_str(), "https://es.example:9200/");
        assert!(matches!(service.auth, Auth::ApiKey(ref key) if key.expose_secret() == "es-key"));
    }

    #[test]
    fn loads_json_config() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.json");
        write(
            &path,
            r#"{
  "current_context": "prod",
  "contexts": {
    "prod": {
      "elasticsearch": {
        "url": "https://es.example:9200",
        "auth": {
          "username": "elastic",
          "password": "changeme"
        }
      }
    }
  }
}"#,
        );

        let config = ConfigFile::load(&path).expect("load config");
        let service = config
            .current()
            .expect("current context")
            .elasticsearch
            .as_ref()
            .expect("Elasticsearch config")
            .resolve()
            .expect("resolve service");

        assert!(matches!(
            service.auth,
            Auth::Basic { ref username, ref password }
                if username == "elastic" && password.expose_secret() == "changeme"
        ));
    }

    #[test]
    fn uses_explicit_path_before_environment_override() {
        let tmp = TempDir::new().expect("temp dir");
        let explicit = tmp.path().join("explicit.yml");
        let env_path = tmp.path().join("env.yml");
        write(
            &explicit,
            "current_context: explicit\ncontexts:\n  explicit:\n    elasticsearch:\n      url: https://explicit.example:9200\n",
        );
        write(
            &env_path,
            "current_context: env\ncontexts:\n  env:\n    elasticsearch:\n      url: https://env.example:9200\n",
        );
        unsafe {
            std::env::set_var("ELASTIC_CLI_CONFIG_FILE", &env_path);
        }

        let config = ConfigFile::load_with_options(Some(&explicit), None).expect("load config");

        assert_eq!(config.current_context_name(), "explicit");
        unsafe {
            std::env::remove_var("ELASTIC_CLI_CONFIG_FILE");
        }
    }

    #[test]
    fn uses_environment_config_override_before_home_discovery() {
        let tmp = TempDir::new().expect("temp dir");
        let home = tmp.path().join("home");
        fs::create_dir(&home).expect("home dir");
        let env_path = tmp.path().join("env.yml");
        write(
            &home.join(".elasticrc.yml"),
            "current_context: home\ncontexts:\n  home:\n    elasticsearch:\n      url: https://home.example:9200\n",
        );
        write(
            &env_path,
            "current_context: env\ncontexts:\n  env:\n    elasticsearch:\n      url: https://env.example:9200\n",
        );
        unsafe {
            std::env::set_var("ELASTIC_CLI_CONFIG_FILE", &env_path);
        }

        let config = ConfigFile::load_with_options(None, Some(&home)).expect("load config");

        assert_eq!(config.current_context_name(), "env");
        unsafe {
            std::env::remove_var("ELASTIC_CLI_CONFIG_FILE");
        }
    }

    #[test]
    fn rejects_executable_config_format() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.js");
        write(&path, "module.exports = {};");

        let err = ConfigFile::load(&path).expect_err("executable config should fail");

        assert!(matches!(err, Error::ExecutableConfigUnsupported { .. }));
    }

    #[test]
    fn rejects_empty_contexts() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(&path, "current_context: prod\ncontexts: {}\n");

        let err = ConfigFile::load(&path).expect_err("empty contexts should fail");

        assert!(matches!(err, Error::InvalidShape(_)));
    }

    #[test]
    fn rejects_missing_context() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n",
        );
        let config = ConfigFile::load(&path).expect("load config");

        let err = config.context("diag").expect_err("missing context should fail");

        assert!(matches!(err, Error::MissingContext { name, .. } if name == "diag"));
    }

    #[test]
    fn rejects_missing_service() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n",
        );
        let config = ConfigFile::load(&path).expect("load config");

        assert!(config.context("prod").expect("prod context").kibana.is_none());
    }

    #[test]
    fn rejects_invalid_service_url() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: file:///tmp/es\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let err = config.validate().expect_err("invalid url should fail");

        assert!(matches!(
            err,
            Error::InvalidServiceUrlScheme {
                context: Some(ref context),
                service: "elasticsearch",
                ..
            } if context == "prod"
        ));

        let err = current_elasticsearch(&config)
            .resolve()
            .expect_err("resolved URL should retain context");
        assert!(matches!(
            err,
            Error::InvalidServiceUrlScheme {
                context: Some(ref context),
                service: "elasticsearch",
                ..
            } if context == "prod"
        ));
    }

    #[test]
    fn resolver_and_url_errors_omit_secrets() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        for (service, secret) in [
            (
                "url: https://es.example:9200\n      auth:\n        username: elastic\n        password: \"hunter2$(oops\"\n",
                "hunter2",
            ),
            (
                "url: https://es.example:9200\n      auth:\n        username: elastic\n        password: \"$(hunter2)\"\n",
                "hunter2",
            ),
            ("url: \"https://elastic:hunter2@es.example:9200$(oops\"\n", "hunter2"),
            ("url: \"ftp://elastic:hunter2@es.example:9200\"\n", "hunter2"),
            ("url: \"http://elastic:hunter2@[::1\"\n", "hunter2"),
        ] {
            write(
                &path,
                &format!("current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      {service}"),
            );
            let config = ConfigFile::load(&path).expect("load config");
            let err = current_elasticsearch(&config)
                .resolve()
                .expect_err("malformed service should fail");
            let message = format!("{err} {err:?}");
            assert!(!message.contains(secret), "{message}");
        }

        assert_eq!(
            super::redact_url("https://user:pass@host:9200/p@th"),
            "https://***@host:9200/p@th"
        );
        assert_eq!(super::redact_url("https://host:9200"), "https://host:9200");
        assert_eq!(super::redact_url("host:9200"), "host:9200");
    }

    #[test]
    fn rejects_invalid_auth_shape() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        username: elastic\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let err = config.validate().expect_err("invalid auth should fail");

        assert!(matches!(
            err,
            Error::InvalidAuth {
                context: Some(ref context),
                service: "elasticsearch",
                ..
            } if context == "prod"
        ));
    }

    #[test]
    fn resolves_environment_expression() {
        let _guard = env_lock().lock().expect("env lock");
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        unsafe {
            std::env::set_var("ELASTICRC_TEST_API_KEY", "env-key");
        }
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(env:ELASTICRC_TEST_API_KEY)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let service = config
            .current()
            .expect("current context")
            .elasticsearch
            .as_ref()
            .expect("Elasticsearch config")
            .resolve()
            .expect("resolve service");

        assert!(matches!(service.auth, Auth::ApiKey(ref key) if key.expose_secret() == "env-key"));
        unsafe {
            std::env::remove_var("ELASTICRC_TEST_API_KEY");
        }
    }

    #[test]
    fn loaded_config_keeps_resolver_backed_secret_raw() {
        let _guard = env_lock().lock().expect("env lock");
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        unsafe {
            std::env::set_var("ELASTICRC_TEST_API_KEY", "env-key");
        }
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(env:ELASTICRC_TEST_API_KEY)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let api_key = config
            .contexts()
            .get("prod")
            .and_then(|context| context.elasticsearch.as_ref())
            .and_then(|service| service.auth.as_ref())
            .and_then(|auth| auth.api_key.as_deref());

        assert_eq!(api_key, Some("$(env:ELASTICRC_TEST_API_KEY)"));
        assert!(!format!("{config:?}").contains("env-key"));
        unsafe {
            std::env::remove_var("ELASTICRC_TEST_API_KEY");
        }
    }

    #[cfg(unix)]
    #[test]
    fn resolving_one_service_does_not_execute_other_service_resolvers() {
        let tmp = TempDir::new().expect("temp dir");
        let marker = tmp.path().join("kibana-resolver-ran");
        let config_path = tmp.path().join(".elasticrc.yml");
        write(
            &config_path,
            &format!(
                "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n    kibana:\n      url: https://kb.example:5601\n      auth:\n        api_key: $(cmd:touch {})\n",
                marker.display()
            ),
        );

        let config = ConfigFile::load(&config_path).expect("load config");
        current_elasticsearch(&config).resolve().expect("resolve Elasticsearch");

        assert!(!marker.exists(), "unselected Kibana resolver must remain lazy");
    }

    #[test]
    fn resolves_file_expression() {
        let tmp = TempDir::new().expect("temp dir");
        let secret = tmp.path().join("secret.txt");
        let config = tmp.path().join(".elasticrc.yml");
        write(&secret, "file-key\n");
        write(
            &config,
            &format!(
                "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(file:{})\n",
                secret.display()
            ),
        );

        let config = ConfigFile::load(&config).expect("load config");
        let service = current_elasticsearch(&config).resolve().expect("resolve service");

        assert!(matches!(service.auth, Auth::ApiKey(ref key) if key.expose_secret() == "file-key"));
    }

    #[test]
    fn resolves_command_expression_without_shell() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(cmd:printf cmd-key)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let service = current_elasticsearch(&config).resolve().expect("resolve service");

        assert!(matches!(service.auth, Auth::ApiKey(ref key) if key.expose_secret() == "cmd-key"));
    }

    #[test]
    fn command_argv_parser_supports_quotes_and_escapes() {
        let argv = parse_command_argv(r#"printf "%s" "my key" escaped\ value"#, "cmd", "field").expect("parse argv");

        assert_eq!(argv, vec!["printf", "%s", "my key", "escaped value"]);
    }

    #[test]
    fn command_argv_parser_preserves_literal_backslashes() {
        let argv =
            parse_command_argv(r#""C:\Program Files\tool.exe" --path=C:\tmp\x"#, "cmd", "field").expect("parse argv");

        assert_eq!(argv, vec![r#"C:\Program Files\tool.exe"#, r#"--path=C:\tmp\x"#]);
    }

    #[test]
    fn command_argv_parser_allows_quoted_metacharacters() {
        let argv = parse_command_argv(r#"printf "%s" "a|b" "x>y""#, "cmd", "field").expect("parse argv");

        assert_eq!(argv, vec!["printf", "%s", "a|b", "x>y"]);
    }

    #[test]
    fn command_output_reader_limits_captured_bytes() {
        let output = vec![b'x'; FILE_RESOLVER_MAX_BYTES as usize + 1];

        let err =
            read_command_output(std::io::Cursor::new(output), "stdout").expect_err("oversized output should fail");

        assert_eq!(err.kind(), ErrorKind::InvalidData);
        assert!(err.to_string().contains("command stdout exceeded"));
    }

    #[test]
    fn file_resolver_reader_limits_captured_bytes() {
        let output = vec![b'x'; FILE_RESOLVER_MAX_BYTES as usize + 1];

        let err = read_file_resolver_value(
            std::io::Cursor::new(output),
            Path::new("secret.txt"),
            "file",
            "auth.api_key",
        )
        .expect_err("oversized output should fail");

        assert!(
            matches!(err, Error::ResolverFailed { message, .. } if message.contains("exceeds resolver size limit"))
        );
    }

    #[test]
    fn rejects_command_expression_that_requires_shell() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(cmd:printf secret | cat)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let err = current_elasticsearch(&config)
            .resolve()
            .expect_err("shell syntax should fail");

        assert!(matches!(err, Error::ShellSyntaxUnsupported { .. }));
    }

    #[test]
    fn rejects_unknown_resolver() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(unknown:value)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let err = current_elasticsearch(&config)
            .resolve()
            .expect_err("unknown resolver should fail");

        assert!(matches!(
            err,
            Error::UnknownResolver { resolver, field }
                if resolver == "unknown" && field == "prod.elasticsearch.auth.api_key"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn loose_inline_secret_config_warns() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: inline-key\n",
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("set permissions");
        let config = ConfigFile::load(&path).expect("load config");

        let warning = inline_secret_permission_warning(&path, &config).expect("warning");

        assert!(warning.contains("contains inline secrets"));
    }

    #[cfg(unix)]
    #[test]
    fn executable_inline_secret_config_warns() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: inline-key\n",
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("set permissions");
        let config = ConfigFile::load(&path).expect("load config");

        let warning = inline_secret_permission_warning(&path, &config).expect("warning");

        assert!(warning.contains("broader than 0600/0400"));
    }

    #[cfg(unix)]
    #[test]
    fn restrictive_inline_secret_config_does_not_warn() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: inline-key\n",
        );
        let config = ConfigFile::load(&path).expect("load config");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("set permissions");

        assert_eq!(inline_secret_permission_warning(&path, &config), None);
    }

    #[cfg(unix)]
    #[test]
    fn resolver_backed_secret_config_does_not_warn() {
        let _guard = env_lock().lock().expect("env lock");
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        unsafe {
            std::env::set_var("ELASTICRC_TEST_API_KEY", "env-key");
        }
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(env:ELASTICRC_TEST_API_KEY)\n",
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("set permissions");
        let config: ConfigFile =
            yaml_serde::from_str(&fs::read_to_string(&path).expect("read config")).expect("parse config");

        assert_eq!(inline_secret_permission_warning(&path, &config), None);
        unsafe {
            std::env::remove_var("ELASTICRC_TEST_API_KEY");
        }
    }
}
