// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    env,
    fmt::Display,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use url::Url;

pub type SecretString = redact::Secret<String>;
const FILE_RESOLVER_MAX_BYTES: u64 = 1024 * 1024;
const COMMAND_RESOLVER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFile {
    pub current_context: String,
    pub contexts: BTreeMap<String, Context>,
}

impl ConfigFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        load_config_file(path)
    }

    pub fn load_from_home(home_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let path = discover_config_path(home_dir.as_ref()).ok_or_else(|| Error::ConfigNotFound {
            home_dir: home_dir.as_ref().to_path_buf(),
        })?;
        Self::load(path)
    }

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

    pub fn validate(&self) -> Result<(), Error> {
        self.validate_shape()?;
        for (context_name, context) in &self.contexts {
            context.validate(context_name)?;
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

    pub fn resolve_expressions(&mut self) -> Result<(), Error> {
        for (context_name, context) in &mut self.contexts {
            context.resolve_expressions(context_name)?;
        }
        Ok(())
    }

    pub fn resolve_service(&self, context_name: &str, kind: ServiceKind) -> Result<ResolvedService, Error> {
        let context = self.contexts.get(context_name).ok_or_else(|| Error::MissingContext {
            name: context_name.to_string(),
            available: self.contexts.keys().cloned().collect(),
        })?;
        let service = context.service(kind).ok_or_else(|| Error::MissingService {
            context: context_name.to_string(),
            service: kind,
        })?;
        let mut service = service.clone();
        service.resolve_expressions(context_name, kind)?;
        service.validate(context_name, kind)?;
        service.resolve(kind)
    }

    pub fn resolve_current_service(&self, kind: ServiceKind) -> Result<ResolvedService, Error> {
        self.resolve_service(&self.current_context, kind)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elasticsearch: Option<ServiceBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kibana: Option<ServiceBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<ServiceBlock>,
}

impl Context {
    pub fn service(&self, kind: ServiceKind) -> Option<&ServiceBlock> {
        match kind {
            ServiceKind::Elasticsearch => self.elasticsearch.as_ref(),
            ServiceKind::Kibana => self.kibana.as_ref(),
            ServiceKind::Cloud => self.cloud.as_ref(),
        }
    }

    fn validate(&self, context_name: &str) -> Result<(), Error> {
        for (kind, service) in [
            (ServiceKind::Elasticsearch, self.elasticsearch.as_ref()),
            (ServiceKind::Kibana, self.kibana.as_ref()),
            (ServiceKind::Cloud, self.cloud.as_ref()),
        ] {
            if let Some(service) = service {
                service.validate(context_name, kind)?;
            }
        }
        Ok(())
    }

    fn resolve_expressions(&mut self, context_name: &str) -> Result<(), Error> {
        for (kind, service) in [
            (ServiceKind::Elasticsearch, self.elasticsearch.as_mut()),
            (ServiceKind::Kibana, self.kibana.as_mut()),
            (ServiceKind::Cloud, self.cloud.as_mut()),
        ] {
            if let Some(service) = service {
                service.resolve_expressions(context_name, kind)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceBlock {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthBlock>,
}

impl ServiceBlock {
    fn validate(&self, context_name: &str, kind: ServiceKind) -> Result<(), Error> {
        let url = Url::parse(&self.url).map_err(|source| Error::InvalidServiceUrl {
            context: context_name.to_string(),
            service: kind,
            value: self.url.clone(),
            source,
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::InvalidServiceUrlScheme {
                context: context_name.to_string(),
                service: kind,
                value: self.url.clone(),
            });
        }
        if let Some(auth) = &self.auth {
            auth.validate(context_name, kind)?;
        }
        Ok(())
    }

    fn resolve(&self, kind: ServiceKind) -> Result<ResolvedService, Error> {
        Ok(ResolvedService {
            kind,
            url: Url::parse(&self.url).map_err(|source| Error::InvalidServiceUrl {
                context: "<resolved>".to_string(),
                service: kind,
                value: self.url.clone(),
                source,
            })?,
            auth: self
                .auth
                .as_ref()
                .map(AuthBlock::resolve)
                .transpose()?
                .unwrap_or_default(),
        })
    }

    fn resolve_expressions(&mut self, context_name: &str, kind: ServiceKind) -> Result<(), Error> {
        self.url = resolve_string_expressions(&self.url, &format!("{context_name}.{kind}.url"))?;
        if let Some(auth) = &mut self.auth {
            auth.resolve_expressions(context_name, kind)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl AuthBlock {
    fn validate(&self, context_name: &str, kind: ServiceKind) -> Result<(), Error> {
        match (&self.api_key, &self.username, &self.password) {
            (Some(_), None, None) | (None, None, None) | (None, Some(_), Some(_)) => Ok(()),
            (Some(_), _, _) => Err(Error::InvalidAuth {
                context: context_name.to_string(),
                service: kind,
                message: "api_key cannot be combined with username or password".to_string(),
            }),
            (None, Some(_), None) | (None, None, Some(_)) => Err(Error::InvalidAuth {
                context: context_name.to_string(),
                service: kind,
                message: "basic authentication requires both username and password".to_string(),
            }),
        }
    }

    fn resolve(&self) -> Result<ResolvedAuth, Error> {
        match (&self.api_key, &self.username, &self.password) {
            (Some(api_key), None, None) => Ok(ResolvedAuth::api_key(api_key.clone())),
            (None, Some(username), Some(password)) => Ok(ResolvedAuth::basic(username.clone(), password.clone())),
            (None, None, None) => Ok(ResolvedAuth::None),
            _ => Err(Error::InvalidShape("invalid auth block".to_string())),
        }
    }

    fn resolve_expressions(&mut self, context_name: &str, kind: ServiceKind) -> Result<(), Error> {
        if let Some(api_key) = &mut self.api_key {
            *api_key = resolve_string_expressions(api_key, &format!("{context_name}.{kind}.auth.api_key"))?;
        }
        if let Some(username) = &mut self.username {
            *username = resolve_string_expressions(username, &format!("{context_name}.{kind}.auth.username"))?;
        }
        if let Some(password) = &mut self.password {
            *password = resolve_string_expressions(password, &format!("{context_name}.{kind}.auth.password"))?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Elasticsearch,
    Kibana,
    Cloud,
}

impl ServiceKind {
    pub fn parse_alias(value: &str) -> Option<Self> {
        match value {
            "elasticsearch" | "es" => Some(Self::Elasticsearch),
            "kibana" | "kb" => Some(Self::Kibana),
            "cloud" => Some(Self::Cloud),
            _ => None,
        }
    }

    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Elasticsearch => "elasticsearch",
            Self::Kibana => "kibana",
            Self::Cloud => "cloud",
        }
    }
}

impl Display for ServiceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.canonical_name())
    }
}

impl FromStr for ServiceKind {
    type Err = UnknownService;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_alias(value).ok_or_else(|| UnknownService(value.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownService(String);

impl Display for UnknownService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown Elastic CLI service '{}'", self.0)
    }
}

impl std::error::Error for UnknownService {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextServiceReference {
    pub context: Option<String>,
    pub service: ServiceKind,
}

impl ContextServiceReference {
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
        Some(Self {
            context,
            service: ServiceKind::parse_alias(service)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedService {
    pub kind: ServiceKind,
    pub url: Url,
    pub auth: ResolvedAuth,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ResolvedAuth {
    ApiKey(SecretString),
    Basic {
        username: String,
        password: SecretString,
    },
    #[default]
    None,
}

impl ResolvedAuth {
    pub fn api_key(api_key: impl Into<String>) -> Self {
        Self::ApiKey(SecretString::new(api_key.into()))
    }

    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: SecretString::new(password.into()),
        }
    }
}

#[derive(Debug)]
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
        service: ServiceKind,
    },
    InvalidServiceUrl {
        context: String,
        service: ServiceKind,
        value: String,
        source: url::ParseError,
    },
    InvalidServiceUrlScheme {
        context: String,
        service: ServiceKind,
        value: String,
    },
    InvalidAuth {
        context: String,
        service: ServiceKind,
        message: String,
    },
    InvalidResolverExpression {
        field: String,
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
            } => write!(
                f,
                "invalid URL for Elastic CLI context '{context}' service '{service}' ({value}): {source}"
            ),
            Self::InvalidServiceUrlScheme {
                context,
                service,
                value,
            } => write!(
                f,
                "invalid URL scheme for Elastic CLI context '{context}' service '{service}' ({value}); expected http or https"
            ),
            Self::InvalidAuth {
                context,
                service,
                message,
            } => write!(
                f,
                "invalid auth for Elastic CLI context '{context}' service '{service}': {message}"
            ),
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

pub fn inline_secret_permission_warning(path: &Path, config: &ConfigFile) -> Option<String> {
    if !config.contains_inline_secret() {
        return None;
    }
    loose_permissions(path).then(|| {
        format!(
            "Warning: Elastic CLI config {} contains inline secrets and has permissions broader than 0600/0400.",
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
        [&self.elasticsearch, &self.kibana, &self.cloud]
            .into_iter()
            .flatten()
            .any(ServiceBlock::contains_inline_secret)
    }
}

impl ServiceBlock {
    fn contains_inline_secret(&self) -> bool {
        self.auth.as_ref().is_some_and(AuthBlock::contains_inline_secret)
    }
}

impl AuthBlock {
    fn contains_inline_secret(&self) -> bool {
        [&self.api_key, &self.password]
            .into_iter()
            .flatten()
            .any(|value| !is_resolver_expression(value))
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
                value: value.to_string(),
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
            value: format!("$({expression})"),
        })?;
    match resolver {
        "env" => env::var(params).map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        }),
        "file" => resolve_file_expression(params, resolver, field),
        "cmd" => resolve_command_expression(params, resolver, field).map(|output| output.trim().to_string()),
        "pass" => resolve_pass_expression(params, resolver, field),
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
    fs::read_to_string(path)
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
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::ResolverFailed {
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
        if let Some(status) = child.try_wait().map_err(|source| Error::ResolverFailed {
            resolver: resolver.to_string(),
            field: field.to_string(),
            message: source.to_string(),
        })? {
            break status;
        }
        if start.elapsed() >= COMMAND_RESOLVER_TIMEOUT {
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

    let stdout = stdout_reader
        .join()
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
    let _stderr = stderr_reader
        .join()
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

fn keyring_store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(target_os = "macos")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "keychain" {
        return Err(format!("resolver '{resolver}' is not supported on macOS"));
    }
    use apple_native_keyring_store::keychain;
    let _guard = keyring_store_lock().lock().map_err(|err| err.to_string())?;
    keyring_core::set_default_store(keychain::Store::new().map_err(|err| err.to_string())?);
    let result = keyring_core::Entry::new(service, account)
        .and_then(|entry| entry.get_password())
        .map_err(|err| err.to_string());
    keyring_core::unset_default_store();
    result
}

#[cfg(target_os = "linux")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "secret_service" {
        return Err(format!("resolver '{resolver}' is not supported on Linux"));
    }
    let _guard = keyring_store_lock().lock().map_err(|err| err.to_string())?;
    keyring_core::set_default_store(zbus_secret_service_keyring_store::Store::new().map_err(|err| err.to_string())?);
    let result = keyring_core::Entry::new(service, account)
        .and_then(|entry| entry.get_password())
        .map_err(|err| err.to_string());
    keyring_core::unset_default_store();
    result
}

#[cfg(target_os = "windows")]
fn resolve_platform_keyring_secret(resolver: &str, service: &str, account: &str) -> Result<String, String> {
    if resolver != "credential_manager" {
        return Err(format!("resolver '{resolver}' is not supported on Windows"));
    }
    let _guard = keyring_store_lock().lock().map_err(|err| err.to_string())?;
    keyring_core::set_default_store(windows_native_keyring_store::Store::new().map_err(|err| err.to_string())?);
    let result = keyring_core::Entry::new(service, account)
        .and_then(|entry| entry.get_password())
        .map_err(|err| err.to_string());
    keyring_core::unset_default_store();
    result
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn resolve_platform_keyring_secret(resolver: &str, _service: &str, _account: &str) -> Result<String, String> {
    Err(format!("resolver '{resolver}' is not supported on this platform"))
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigFile, ContextServiceReference, Error, FILE_RESOLVER_MAX_BYTES, ResolvedAuth, ServiceKind,
        discover_config_path, inline_secret_permission_warning, parse_command_argv, read_command_output,
    };
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::{
        fs,
        io::ErrorKind,
        path::Path,
        str::FromStr,
        sync::{Mutex, OnceLock},
    };
    use tempfile::TempDir;

    fn write(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write config");
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn service_kind_parses_supported_aliases() {
        assert_eq!(ServiceKind::from_str("elasticsearch"), Ok(ServiceKind::Elasticsearch));
        assert_eq!(ServiceKind::from_str("es"), Ok(ServiceKind::Elasticsearch));
        assert_eq!(ServiceKind::from_str("kibana"), Ok(ServiceKind::Kibana));
        assert_eq!(ServiceKind::from_str("kb"), Ok(ServiceKind::Kibana));
        assert_eq!(ServiceKind::from_str("cloud"), Ok(ServiceKind::Cloud));
    }

    #[test]
    fn service_kind_rejects_unsupported_aliases() {
        assert!(ServiceKind::from_str("ls").is_err());
        assert!(ServiceKind::from_str("logstash").is_err());
    }

    #[test]
    fn context_service_reference_parses_active_context_service() {
        assert_eq!(
            ContextServiceReference::parse(".es"),
            Some(ContextServiceReference {
                context: None,
                service: ServiceKind::Elasticsearch,
            })
        );
    }

    #[test]
    fn context_service_reference_parses_named_context_from_rightmost_segment() {
        assert_eq!(
            ContextServiceReference::parse(".prod.us-west.es"),
            Some(ContextServiceReference {
                context: Some("prod.us-west".to_string()),
                service: ServiceKind::Elasticsearch,
            })
        );
    }

    #[test]
    fn context_service_reference_ignores_unknown_service_segments() {
        assert_eq!(ContextServiceReference::parse(".prod.ls"), None);
        assert_eq!(ContextServiceReference::parse(".unknown"), None);
        assert_eq!(ContextServiceReference::parse("./.es"), None);
    }

    #[test]
    fn resolved_auth_debug_redacts_api_key() {
        let auth = super::ResolvedAuth::api_key("super-secret");
        let rendered = format!("{auth:?}");

        assert!(rendered.contains("[REDACTED"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn resolved_auth_debug_redacts_basic_password() {
        let auth = super::ResolvedAuth::basic("elastic", "super-secret");
        let rendered = format!("{auth:?}");

        assert!(rendered.contains("elastic"));
        assert!(rendered.contains("[REDACTED"));
        assert!(!rendered.contains("super-secret"));
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
            .resolve_service("prod", ServiceKind::Elasticsearch)
            .expect("resolve service");

        assert_eq!(service.url.as_str(), "https://es.example:9200/");
        assert!(matches!(service.auth, ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "es-key"));
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
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect("resolve service");

        assert!(matches!(
            service.auth,
            ResolvedAuth::Basic { ref username, ref password }
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

        assert_eq!(config.current_context, "explicit");
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

        assert_eq!(config.current_context, "env");
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

        let err = config
            .resolve_service("diag", ServiceKind::Elasticsearch)
            .expect_err("missing context should fail");

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

        let err = config
            .resolve_service("prod", ServiceKind::Kibana)
            .expect_err("missing service should fail");

        assert!(matches!(
            err,
            Error::MissingService {
                service: ServiceKind::Kibana,
                ..
            }
        ));
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
        let err = config
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect_err("invalid url should fail");

        assert!(matches!(
            err,
            Error::InvalidServiceUrlScheme {
                service: ServiceKind::Elasticsearch,
                ..
            }
        ));
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
        let err = config
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect_err("invalid auth should fail");

        assert!(matches!(
            err,
            Error::InvalidAuth {
                service: ServiceKind::Elasticsearch,
                ..
            }
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
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect("resolve service");

        assert!(matches!(service.auth, ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "env-key"));
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
            .contexts
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
        let service = config
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect("resolve service");

        assert!(matches!(service.auth, ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "file-key"));
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
        let service = config
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect("resolve service");

        assert!(matches!(service.auth, ResolvedAuth::ApiKey(ref key) if key.expose_secret() == "cmd-key"));
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
    fn rejects_command_expression_that_requires_shell() {
        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join(".elasticrc.yml");
        write(
            &path,
            "current_context: prod\ncontexts:\n  prod:\n    elasticsearch:\n      url: https://es.example:9200\n      auth:\n        api_key: $(cmd:printf secret | cat)\n",
        );

        let config = ConfigFile::load(&path).expect("load config");
        let err = config
            .resolve_current_service(ServiceKind::Elasticsearch)
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
        let err = config
            .resolve_current_service(ServiceKind::Elasticsearch)
            .expect_err("unknown resolver should fail");

        assert!(matches!(err, Error::UnknownResolver { resolver, .. } if resolver == "unknown"));
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
