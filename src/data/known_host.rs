// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::keystore::upsert_secret_auth_batch;
use crate::data::{Auth, Product, SecretAuth, get_keystore_password, resolve_secret_auth as resolve_secret_by_id};
use eyre::{Result, eyre};
#[cfg(test)]
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_yaml;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    collections::BTreeMap,
    env,
    fmt::{Display, Formatter},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;

const DEFAULT_TEMPLATE_PRODUCT: &str = "elasticsearch";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostRole {
    Collect,
    Send,
    View,
}

impl std::fmt::Display for HostRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Collect => write!(f, "collect"),
            Self::Send => write!(f, "send"),
            Self::View => write!(f, "view"),
        }
    }
}

impl FromStr for HostRole {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "collect" => Ok(Self::Collect),
            "send" => Ok(Self::Send),
            "view" => Ok(Self::View),
            _ => Err(eyre!("Unknown host role '{s}'")),
        }
    }
}

fn default_collect_roles() -> Vec<HostRole> {
    vec![HostRole::Collect]
}

fn product_cli_name(product: &Product) -> &'static str {
    match product {
        Product::Agent => "agent",
        Product::ECE => "ece",
        Product::ECK => "eck",
        Product::ElasticCloudHosted => "elastic-cloud-hosted",
        Product::Elasticsearch => "elasticsearch",
        Product::Kibana => "kibana",
        Product::KubernetesPlatform => "kubernetes-platform",
        Product::Logstash => "logstash",
        Product::Unknown => "unknown",
    }
}

fn roles_is_default_collect(roles: &[HostRole]) -> bool {
    roles.len() == 1 && roles[0] == HostRole::Collect
}

fn default_false() -> bool {
    false
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElasticCloud {
    ElasticGovCloudAdmin,
    ElasticCloudAdmin,
    ElasticCloud,
}

impl TryFrom<&Url> for ElasticCloud {
    type Error = String;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if url.domain() == Some("admin.us-gov-east-1.aws.elastic-cloud.com") {
            Ok(ElasticCloud::ElasticGovCloudAdmin)
        } else if url.domain() == Some("admin.found.no") {
            Ok(ElasticCloud::ElasticCloudAdmin)
        } else if url.domain() == Some("cloud.elastic.co") {
            Ok(ElasticCloud::ElasticCloud)
        } else {
            Err(String::from("Not an elastic Cloud URL"))
        }
    }
}

impl Display for ElasticCloud {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ElasticCloud::ElasticGovCloudAdmin => write!(f, "ElasticGovCloudAdmin"),
            ElasticCloud::ElasticCloudAdmin => write!(f, "ElasticCloudAdmin"),
            ElasticCloud::ElasticCloud => write!(f, "ElasticCloud"),
        }
    }
}

pub struct KnownHostBuilder {
    accept_invalid_certs: bool,
    apikey: Option<String>,
    product: Product,
    cloud_id: Option<ElasticCloud>,
    password: Option<String>,
    roles: Vec<HostRole>,
    secret: Option<String>,
    url: Option<Url>,
    url_template: Option<String>,
    username: Option<String>,
    viewer: Option<String>,
}

impl KnownHostBuilder {
    pub fn new(url: Url) -> Self {
        KnownHostBuilder {
            accept_invalid_certs: false,
            apikey: None,
            product: Product::Elasticsearch,
            cloud_id: None,
            password: None,
            roles: default_collect_roles(),
            secret: None,
            url: Some(url),
            url_template: None,
            username: None,
            viewer: None,
        }
    }

    pub fn new_template(url_template: String) -> Self {
        KnownHostBuilder {
            accept_invalid_certs: false,
            apikey: None,
            product: Product::Unknown,
            cloud_id: None,
            password: None,
            roles: default_collect_roles(),
            secret: None,
            url: None,
            url_template: Some(url_template),
            username: None,
            viewer: None,
        }
    }

    pub fn accept_invalid_certs(self, accept_invalid_certs: bool) -> Self {
        Self {
            accept_invalid_certs,
            ..self
        }
    }

    pub fn apikey(self, apikey: Option<String>) -> Self {
        Self { apikey, ..self }
    }

    pub fn password(self, password: Option<String>) -> Self {
        Self { password, ..self }
    }

    pub fn product(self, product: Product) -> Self {
        Self { product, ..self }
    }

    pub fn secret(self, secret: Option<String>) -> Self {
        Self { secret, ..self }
    }

    pub fn roles(self, roles: Vec<HostRole>) -> Self {
        Self { roles, ..self }
    }

    pub fn username(self, username: Option<String>) -> Self {
        Self { username, ..self }
    }

    pub fn viewer(self, viewer: Option<String>) -> Self {
        Self { viewer, ..self }
    }

    fn update_cloud_api_path(&mut self) {
        let Some(mut url) = self.url.clone() else {
            self.cloud_id = None;
            return;
        };
        self.cloud_id = ElasticCloud::try_from(&url).ok();
        if self.cloud_id.is_none() {
            return;
        }

        // Desired URL format is https://{domain}/api/v1/deployments/{deployment_id}/elasticsearch/elasticsearch/proxy/
        let deployment_id = url.clone();
        let deployment_id = deployment_id
            .path()
            .split('/')
            .skip_while(|segment| *segment != "deployments")
            .nth(1)
            .unwrap_or("");
        let rendered = match self.product {
            Product::Elasticsearch => {
                let template = match url.domain() {
                    Some("admin.found.no") => {
                        "https://admin.found.no/api/v1/deployments/{id}/elasticsearch/main-{product}/proxy/"
                    }
                    Some("cloud.elastic.co") => {
                        "https://cloud.elastic.co/api/v1/deployments/{id}/elasticsearch/{product}/proxy/"
                    }
                    Some("admin.us-gov-east-1.aws.elastic-cloud.com") => {
                        "https://admin.us-gov-east-1.aws.elastic-cloud.com/api/v1/deployments/{id}/elasticsearch/{product}/proxy/"
                    }
                    _ => "",
                };
                if template.is_empty() {
                    None
                } else {
                    render_url_template_url(template, deployment_id, DEFAULT_TEMPLATE_PRODUCT).ok()
                }
            }
            _ => None,
        };
        if let Some(mut rendered) = rendered {
            if url.query().is_some() {
                rendered
                    .query_pairs_mut()
                    .extend_pairs(url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())));
            }
            url = rendered;
        }

        tracing::debug!("Updated Cloud API URL: {}", url);
        self.url = Some(url);
    }

    pub fn build(mut self) -> Result<KnownHost> {
        self.update_cloud_api_path();
        KnownHost::from_parts(KnownHostParts {
            accept_invalid_certs: self.accept_invalid_certs,
            app: self.product,
            cloud_id: self.cloud_id,
            roles: self.roles,
            secret: self.secret,
            viewer: self.viewer,
            url: self.url,
            url_template: self.url_template,
            legacy_apikey: self.apikey,
            legacy_username: self.username,
            legacy_password: self.password,
        })
    }

    pub fn build_with_secret_auth(mut self, _secret_auth: SecretAuth) -> Result<KnownHost> {
        self.update_cloud_api_path();

        if self.apikey.is_some() || self.username.is_some() || self.password.is_some() {
            return Err(eyre!(
                "Invalid KnownHost configuration: explicit credentials conflict with secret auth"
            ));
        }

        let secret = self
            .secret
            .take()
            .ok_or_else(|| eyre!("Invalid KnownHost configuration: missing secret reference"))?;

        KnownHost::from_parts(KnownHostParts {
            accept_invalid_certs: self.accept_invalid_certs,
            app: self.product,
            cloud_id: self.cloud_id,
            roles: self.roles,
            secret: Some(secret),
            viewer: self.viewer,
            url: self.url,
            url_template: self.url_template,
            legacy_apikey: None,
            legacy_username: None,
            legacy_password: None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownHostCliUpdate {
    pub accept_invalid_certs: Option<bool>,
    pub apikey: Option<String>,
    pub password: Option<String>,
    pub roles: Option<Vec<HostRole>>,
    pub secret: Option<String>,
    pub username: Option<String>,
}

impl KnownHostCliUpdate {
    pub fn is_empty(&self) -> bool {
        self.accept_invalid_certs.is_none()
            && self.apikey.is_none()
            && self.password.is_none()
            && self.roles.is_none()
            && self.secret.is_none()
            && self.username.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHostSummary {
    pub app: String,
    pub name: String,
    pub secret: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHost {
    pub accept_invalid_certs: bool,
    pub app: Product,
    pub cloud_id: Option<ElasticCloud>,
    pub roles: Vec<HostRole>,
    pub secret: Option<String>,
    pub viewer: Option<String>,
    pub url: Option<Url>,
    pub url_template: Option<String>,
    pub legacy_apikey: Option<String>,
    pub legacy_username: Option<String>,
    pub legacy_password: Option<String>,
}

#[derive(Serialize)]
struct FlatKnownHostRef<'a> {
    #[serde(default = "default_false", skip_serializing_if = "is_false")]
    accept_invalid_certs: bool,
    app: &'a Product,
    #[serde(skip_serializing_if = "Option::is_none")]
    cloud_id: &'a Option<ElasticCloud>,
    #[serde(default = "default_collect_roles", skip_serializing_if = "roles_is_default_collect")]
    roles: &'a Vec<HostRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: &'a Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    viewer: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: &'a Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url_template: &'a Option<String>,
}

#[cfg(test)]
#[derive(Serialize)]
#[serde(tag = "auth")]
enum LegacyKnownHostRef<'a> {
    ApiKey {
        #[serde(default = "default_false", skip_serializing_if = "is_false")]
        accept_invalid_certs: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        apikey: &'a Option<String>,
        app: &'a Product,
        #[serde(skip_serializing_if = "Option::is_none")]
        cloud_id: &'a Option<ElasticCloud>,
        #[serde(default = "default_collect_roles", skip_serializing_if = "roles_is_default_collect")]
        roles: &'a Vec<HostRole>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: &'a Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: &'a Option<String>,
        url: &'a Url,
    },
    Basic {
        #[serde(default = "default_false", skip_serializing_if = "is_false")]
        accept_invalid_certs: bool,
        app: &'a Product,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: &'a Option<String>,
        #[serde(default = "default_collect_roles", skip_serializing_if = "roles_is_default_collect")]
        roles: &'a Vec<HostRole>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: &'a Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        viewer: &'a Option<String>,
        url: &'a Url,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        username: &'a Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum KnownHostWire {
    Legacy(LegacyKnownHostWire),
    Flat(FlatKnownHostWire),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FlatKnownHostWire {
    #[serde(default = "default_false")]
    accept_invalid_certs: bool,
    app: Product,
    #[serde(default)]
    cloud_id: Option<ElasticCloud>,
    #[serde(default = "default_collect_roles")]
    roles: Vec<HostRole>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    viewer: Option<String>,
    #[serde(default)]
    url: Option<Url>,
    #[serde(default)]
    url_template: Option<String>,
    #[serde(default)]
    apikey: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "auth", deny_unknown_fields)]
enum LegacyKnownHostWire {
    ApiKey {
        #[serde(default = "default_false")]
        accept_invalid_certs: bool,
        #[serde(default)]
        apikey: Option<String>,
        app: Product,
        #[serde(default)]
        cloud_id: Option<ElasticCloud>,
        #[serde(default = "default_collect_roles")]
        roles: Vec<HostRole>,
        #[serde(default)]
        secret: Option<String>,
        #[serde(default)]
        viewer: Option<String>,
        url: Url,
    },
    Basic {
        #[serde(default = "default_false")]
        accept_invalid_certs: bool,
        app: Product,
        #[serde(default)]
        password: Option<String>,
        #[serde(default = "default_collect_roles")]
        roles: Vec<HostRole>,
        #[serde(default)]
        secret: Option<String>,
        #[serde(default)]
        viewer: Option<String>,
        url: Url,
        #[serde(default)]
        username: Option<String>,
    },
    #[serde(alias = "None")]
    NoAuth {
        #[serde(default = "default_false")]
        accept_invalid_certs: bool,
        app: Product,
        #[serde(default = "default_collect_roles")]
        roles: Vec<HostRole>,
        #[serde(default)]
        viewer: Option<String>,
        url: Url,
    },
}

struct KnownHostParts {
    accept_invalid_certs: bool,
    app: Product,
    cloud_id: Option<ElasticCloud>,
    roles: Vec<HostRole>,
    secret: Option<String>,
    viewer: Option<String>,
    url: Option<Url>,
    url_template: Option<String>,
    legacy_apikey: Option<String>,
    legacy_username: Option<String>,
    legacy_password: Option<String>,
}

impl Serialize for KnownHost {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        FlatKnownHostRef {
            accept_invalid_certs: self.accept_invalid_certs,
            app: &self.app,
            cloud_id: &self.cloud_id,
            roles: &self.roles,
            secret: &self.secret,
            viewer: &self.viewer,
            url: &self.url,
            url_template: &self.url_template,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KnownHost {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = KnownHostWire::deserialize(deserializer)?;
        let host = match wire {
            KnownHostWire::Legacy(legacy) => Self::from_legacy_wire(legacy),
            KnownHostWire::Flat(flat) => Self::from_flat_wire(flat),
        };
        host.map_err(serde::de::Error::custom)
    }
}

impl KnownHost {
    fn from_parts(parts: KnownHostParts) -> Result<Self> {
        let KnownHostParts {
            accept_invalid_certs,
            app,
            cloud_id,
            roles,
            secret,
            viewer,
            url,
            url_template,
            legacy_apikey,
            legacy_username,
            legacy_password,
        } = parts;
        match (&legacy_apikey, &legacy_username, &legacy_password) {
            (Some(_), None, None) => {}
            (None, Some(_), Some(_)) | (None, None, None) => {}
            (Some(_), _, _) => {
                return Err(eyre!(
                    "Invalid KnownHost configuration: API key auth cannot be combined with username or password"
                ));
            }
            (None, Some(_), None) | (None, None, Some(_)) => {
                return Err(eyre!(
                    "Invalid KnownHost configuration: basic auth requires both username and password"
                ));
            }
        }

        match (&url, &url_template) {
            (Some(_), None) => {}
            (None, Some(template)) => validate_url_template(template)?,
            (Some(_), Some(_)) => {
                return Err(eyre!(
                    "Invalid KnownHost configuration: `url` and `url_template` are mutually exclusive"
                ));
            }
            (None, None) => {
                return Err(eyre!(
                    "Invalid KnownHost configuration: one of `url` or `url_template` is required"
                ));
            }
        }

        let cloud_id = url
            .as_ref()
            .and_then(|url| cloud_id.clone().or_else(|| ElasticCloud::try_from(url).ok()));
        let url = url.map(|url| normalize_elastic_cloud_proxy_url(url, cloud_id.as_ref()));

        Ok(Self {
            accept_invalid_certs,
            app,
            cloud_id,
            roles,
            secret,
            viewer,
            url,
            url_template,
            legacy_apikey,
            legacy_username,
            legacy_password,
        })
    }

    fn from_flat_wire(wire: FlatKnownHostWire) -> Result<Self> {
        Self::from_parts(KnownHostParts {
            accept_invalid_certs: wire.accept_invalid_certs,
            app: wire.app,
            cloud_id: wire.cloud_id,
            roles: wire.roles,
            secret: wire.secret.clone(),
            viewer: wire.viewer,
            url: wire.url,
            url_template: wire.url_template,
            legacy_apikey: wire.apikey,
            legacy_username: wire.username,
            legacy_password: wire.password,
        })
    }

    fn from_legacy_wire(wire: LegacyKnownHostWire) -> Result<Self> {
        match wire {
            LegacyKnownHostWire::ApiKey {
                accept_invalid_certs,
                apikey,
                app,
                cloud_id,
                roles,
                secret,
                viewer,
                url,
            } => Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id,
                roles,
                secret: secret.clone(),
                viewer,
                url: Some(url),
                url_template: None,
                legacy_apikey: apikey,
                legacy_username: None,
                legacy_password: None,
            }),
            LegacyKnownHostWire::Basic {
                accept_invalid_certs,
                app,
                password,
                roles,
                secret,
                viewer,
                url,
                username,
            } => Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id: None,
                roles,
                secret: secret.clone(),
                viewer,
                url: Some(url),
                url_template: None,
                legacy_apikey: None,
                legacy_username: username,
                legacy_password: password,
            }),
            LegacyKnownHostWire::NoAuth {
                accept_invalid_certs,
                app,
                roles,
                viewer,
                url,
            } => Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id: None,
                roles,
                secret: None,
                viewer,
                url: Some(url),
                url_template: None,
                legacy_apikey: None,
                legacy_username: None,
                legacy_password: None,
            }),
        }
    }

    fn has_legacy_plaintext_auth(&self) -> bool {
        self.legacy_apikey.is_some() || self.legacy_username.is_some() || self.legacy_password.is_some()
    }

    fn clear_legacy_plaintext_auth(&mut self) {
        self.legacy_apikey = None;
        self.legacy_username = None;
        self.legacy_password = None;
    }

    fn auth_label(&self) -> &'static str {
        if self.secret.is_some() {
            "secret"
        } else if self.legacy_apikey.is_some() {
            "apikey"
        } else if self.legacy_username.is_some() || self.legacy_password.is_some() {
            "basic"
        } else {
            "none"
        }
    }

    pub fn new_no_auth(
        app: Product,
        url: Url,
        roles: Vec<HostRole>,
        viewer: Option<String>,
        accept_invalid_certs: bool,
    ) -> Self {
        Self::from_parts(KnownHostParts {
            accept_invalid_certs,
            app,
            cloud_id: ElasticCloud::try_from(&url).ok(),
            roles,
            secret: None,
            viewer,
            url: Some(url),
            url_template: None,
            legacy_apikey: None,
            legacy_username: None,
            legacy_password: None,
        })
        .expect("valid no-auth host")
    }

    pub fn new_legacy_apikey(
        app: Product,
        url: Url,
        roles: Vec<HostRole>,
        viewer: Option<String>,
        accept_invalid_certs: bool,
        secret: Option<String>,
        apikey: Option<String>,
    ) -> Self {
        Self::from_parts(KnownHostParts {
            accept_invalid_certs,
            app,
            cloud_id: ElasticCloud::try_from(&url).ok(),
            roles,
            secret: secret.clone(),
            viewer,
            url: Some(url),
            url_template: None,
            legacy_apikey: apikey,
            legacy_username: None,
            legacy_password: None,
        })
        .expect("valid legacy api key host")
    }

    pub fn new_legacy_basic(
        app: Product,
        url: Url,
        roles: Vec<HostRole>,
        viewer: Option<String>,
        accept_invalid_certs: bool,
        secret: Option<String>,
        credentials: Option<(String, String)>,
    ) -> Self {
        let (legacy_username, legacy_password) = credentials
            .map(|(username, password)| (Some(username), Some(password)))
            .unwrap_or((None, None));
        Self::from_parts(KnownHostParts {
            accept_invalid_certs,
            app,
            cloud_id: ElasticCloud::try_from(&url).ok(),
            roles,
            secret: secret.clone(),
            viewer,
            url: Some(url),
            url_template: None,
            legacy_apikey: None,
            legacy_username,
            legacy_password,
        })
        .expect("valid legacy basic host")
    }

    pub fn app(&self) -> &Product {
        &self.app
    }

    pub fn get_url(&self) -> Result<Url> {
        self.url
            .clone()
            .ok_or_else(|| eyre!("Template-backed hosts must be resolved into a concrete URL before runtime use"))
    }

    pub fn concrete_url(&self) -> Option<&Url> {
        self.url.as_ref()
    }

    pub fn url_template(&self) -> Option<&str> {
        self.url_template.as_deref()
    }

    pub fn is_template(&self) -> bool {
        self.url_template.is_some()
    }

    pub fn transport_display(&self) -> String {
        match (&self.url, &self.url_template) {
            (Some(url), None) => url.to_string(),
            (None, Some(url_template)) => url_template.clone(),
            _ => String::new(),
        }
    }

    pub fn roles(&self) -> &[HostRole] {
        &self.roles
    }

    pub fn has_role(&self, role: HostRole) -> bool {
        self.roles().contains(&role)
    }

    pub fn viewer(&self) -> Option<&str> {
        self.viewer.as_deref()
    }

    pub fn accept_invalid_certs(&self) -> bool {
        self.accept_invalid_certs
    }

    pub fn cloud_id(&self) -> Option<&ElasticCloud> {
        self.cloud_id.as_ref()
    }

    pub fn requires_keystore_secret(&self) -> bool {
        self.secret.is_some()
    }

    pub fn secret_reference(&self) -> Option<&str> {
        self.secret.as_deref()
    }

    pub fn get_auth(&self) -> Result<Auth> {
        resolve_auth_with_precedence(
            &self.secret,
            self.legacy_apikey.clone(),
            self.legacy_username.clone().zip(self.legacy_password.clone()),
        )
    }

    pub fn template_guidance(name: &str) -> String {
        format!(
            "Host '{name}' is template-backed and requires an `id` plus an optional `product`. Use `{name}://<id>[/<product>]` and omit `product` to default to `{DEFAULT_TEMPLATE_PRODUCT}`."
        )
    }

    pub fn resolve_template_reference(reference: &str) -> Result<Option<Self>> {
        let Some(parsed) = parse_template_reference(reference)? else {
            return Ok(None);
        };
        let template_name = parsed.template_name.clone();
        let host = Self::get_known(&template_name)
            .ok_or_else(|| eyre!("Template host '{template_name}' not found"))?;
        if !host.is_template() {
            return Err(eyre!("Saved host '{template_name}' is not template-backed"));
        }
        host.render_template_reference(&parsed.id, parsed.product.as_deref())
            .map(Some)
    }

    pub fn render_template_reference(&self, id: &str, product: Option<&str>) -> Result<Self> {
        let url_template = self
            .url_template()
            .ok_or_else(|| eyre!("KnownHost is not template-backed"))?;
        let id = id.trim();
        if id.is_empty() {
            return Err(eyre!("Template reference is missing required `id`"));
        }

        let product = product.unwrap_or(DEFAULT_TEMPLATE_PRODUCT).trim().to_ascii_lowercase();
        let app = Product::from_str(&product)
            .map_err(|_| eyre!("Unsupported template product '{product}'"))?;
        let url = render_url_template_url(url_template, id, &product)?;

        let mut builder = KnownHostBuilder::new(url)
            .product(app)
            .accept_invalid_certs(self.accept_invalid_certs)
            .roles(self.roles.clone())
            .viewer(self.viewer.clone())
            .apikey(self.legacy_apikey.clone())
            .username(self.legacy_username.clone())
            .password(self.legacy_password.clone())
            .secret(self.secret.clone());
        if self.roles.is_empty() {
            builder = builder.roles(default_collect_roles());
        }
        builder.build()
    }

    fn validate_template_host_name(&self, host_name: &str) -> Result<()> {
        if self.is_template() {
            validate_template_host_name(host_name)?;
        }
        Ok(())
    }

    pub fn save(mut self, name: &str) -> Result<String> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let mut hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                tracing::error!("Error parsing hosts.yml: {}", e);
                return Err(eyre!("Error parsing hosts.yml"));
            }
        };
        self.normalize_and_validate_roles(name)?;
        self.validate_template_host_name(name)?;
        if self.secret.is_some() {
            self.clear_legacy_plaintext_auth();
        } else if self.has_legacy_plaintext_auth() {
            return Err(eyre!(
                "Host '{name}' requires a secret reference before it can be saved. Use `--secret <id>` to persist auth, or `--nosave` for transient validation."
            ));
        }
        hosts.insert(name.to_owned(), self);
        KnownHost::write_hosts_yml(&hosts)
    }

    pub fn merge_cli_update(&self, update: &KnownHostCliUpdate, secret_auth: Option<SecretAuth>) -> Result<Self> {
        let app = self.app().clone();
        let url = self.concrete_url().cloned();
        let url_template = self.url_template.clone();
        let viewer = self.viewer().map(str::to_string);
        let roles = update.roles.clone().unwrap_or_else(|| self.roles().to_vec());
        let accept_invalid_certs = update
            .accept_invalid_certs
            .unwrap_or_else(|| self.accept_invalid_certs());

        if let Some(secret_id) = &update.secret {
            let _secret_auth = secret_auth
                .ok_or_else(|| eyre!("Invalid KnownHost configuration: missing secret auth for secret update"))?;
            return Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id: url.as_ref().and_then(|url| ElasticCloud::try_from(url).ok()),
                roles,
                secret: Some(secret_id.clone()),
                viewer,
                url,
                url_template,
                legacy_apikey: None,
                legacy_username: None,
                legacy_password: None,
            });
        }

        if let Some(apikey) = &update.apikey {
            return Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id: url.as_ref().and_then(|url| ElasticCloud::try_from(url).ok()),
                roles,
                secret: None,
                viewer,
                url,
                url_template,
                legacy_apikey: Some(apikey.clone()),
                legacy_username: None,
                legacy_password: None,
            });
        }

        if update.username.is_some() || update.password.is_some() {
            let username = update.username.clone();
            let password = update.password.clone();
            if username.is_none() || password.is_none() {
                return Err(eyre!(
                    "Invalid Basic auth update: either provide a secret reference or both username and password"
                ));
            }
            return Self::from_parts(KnownHostParts {
                accept_invalid_certs,
                app,
                cloud_id: url.as_ref().and_then(|url| ElasticCloud::try_from(url).ok()),
                roles,
                secret: None,
                viewer,
                url,
                url_template,
                legacy_apikey: None,
                legacy_username: username,
                legacy_password: password,
            });
        }

        let mut merged = self.clone();
        merged.set_roles(roles);
        merged.set_accept_invalid_certs(accept_invalid_certs);
        Ok(merged)
    }

    pub fn get_known(host: &String) -> Option<Self> {
        // parse the ~/.esdiag/hosts.yml file into a HashMap<String, Host>
        let hosts = match KnownHost::parse_hosts_yml() {
            Ok(hosts) => hosts,
            Err(e) => {
                tracing::error!("Error parsing hosts.yml: {}", e);
                return None;
            }
        };
        tracing::debug!(
            "Known hosts: {}",
            hosts.clone().into_keys().collect::<Vec<String>>().join(", ")
        );
        hosts.get(host).cloned()
    }

    pub fn remove_saved(name: &str) -> Result<String> {
        let mut hosts = Self::parse_hosts_yml()?;
        if hosts.remove(name).is_none() {
            return Err(eyre!("Host '{name}' not found"));
        }
        Self::write_hosts_yml(&hosts)
    }

    pub fn has_legacy_secret(&self) -> bool {
        self.has_legacy_plaintext_auth()
    }

    pub fn set_secret_reference(&mut self, secret_id: String) {
        self.secret = Some(secret_id);
        self.clear_legacy_plaintext_auth();
    }

    pub fn migrate_hosts_to_keystore(keystore_password: &str) -> Result<(usize, usize)> {
        let mut hosts = Self::parse_hosts_yml()?;
        let mut migrated = 0_usize;
        let mut unchanged = 0_usize;
        let total = hosts.len();
        let mut pending = Vec::new();

        tracing::info!("Starting keystore migration for {total} host(s).");

        for (index, (name, host)) in hosts.iter_mut().enumerate() {
            match host.legacy_auth() {
                Some(auth) => {
                    tracing::debug!(
                        "Preparing host {}/{} for keystore migration: {}",
                        index + 1,
                        total,
                        name
                    );
                    pending.push((name.clone(), auth));
                    host.set_secret_reference(name.clone());
                    migrated += 1;
                }
                None => {
                    tracing::debug!(
                        "Skipping host {}/{} without legacy credentials: {}",
                        index + 1,
                        total,
                        name
                    );
                    unchanged += 1;
                }
            }
        }

        if !pending.is_empty() {
            if let Some(backup_path) = Self::backup_hosts_yml()? {
                tracing::info!("Backed up hosts.yml to '{}'.", backup_path.display());
            }
            tracing::info!(
                "Writing {} migrated secret(s) to keystore in a single batch.",
                pending.len()
            );
            upsert_secret_auth_batch(pending, keystore_password)?;
        }

        tracing::info!("Writing migrated host references back to hosts.yml.");
        Self::write_hosts_yml(&hosts)?;
        Ok((migrated, unchanged))
    }

    pub fn list_by_role(role: HostRole) -> Result<Vec<String>> {
        let hosts = Self::parse_hosts_yml()?;
        let mut names: Vec<String> = hosts
            .iter()
            .filter_map(|(name, host)| host.has_role(role.clone()).then_some(name.clone()))
            .collect();
        names.sort();
        Ok(names)
    }

    fn legacy_auth(&self) -> Option<SecretAuth> {
        match (
            self.legacy_apikey.clone(),
            self.legacy_username.clone(),
            self.legacy_password.clone(),
        ) {
            (Some(apikey), None, None) => Some(SecretAuth::ApiKey { apikey }),
            (None, Some(username), Some(password)) => Some(SecretAuth::Basic { username, password }),
            _ => None,
        }
    }

    pub fn list_all() -> Option<Vec<String>> {
        let hosts = KnownHost::parse_hosts_yml().ok()?;
        let mut names: Vec<String> = hosts.keys().cloned().collect();
        names.sort();
        Some(names)
    }

    pub fn list_saved_summaries() -> Result<Vec<KnownHostSummary>> {
        let hosts = Self::parse_hosts_yml()?;
        Ok(hosts
            .into_iter()
            .map(|(name, host)| KnownHostSummary {
                app: product_cli_name(host.app()).to_string(),
                name,
                secret: host.secret_reference().map(str::to_string),
            })
            .collect())
    }

    pub fn from_url(url: &Url) -> Self {
        KnownHost {
            accept_invalid_certs: false,
            app: Product::Elasticsearch,
            cloud_id: ElasticCloud::try_from(url).ok(),
            roles: default_collect_roles(),
            secret: None,
            viewer: None,
            url: Some(url.clone()),
            url_template: None,
            legacy_apikey: None,
            legacy_username: None,
            legacy_password: None,
        }
    }

    pub fn get_hosts_path() -> PathBuf {
        match env::var("ESDIAG_HOSTS") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                let home_dir = match std::env::consts::OS {
                    "windows" => std::env::var("USERPROFILE").expect("Failed to get USERPROFILE"),
                    "linux" | "macos" => std::env::var("HOME").expect("Failed to get HOME"),
                    os => panic!("Unknown home directory for operating system: {os} "),
                };
                let home_dir = PathBuf::from(home_dir);
                // Check if the `.esdiag` directory exists, if not, create it
                let esdiag = home_dir.join(".esdiag");
                if !esdiag.exists() {
                    std::fs::create_dir_all(&esdiag).expect("Failed to create ~/.esdiag directory");
                }

                home_dir.join(".esdiag").join("hosts.yml")
            }
        }
    }

    fn backup_path(path: &Path) -> Result<PathBuf> {
        let file_name = path
            .file_name()
            .ok_or_else(|| eyre!("Path '{}' has no file name", path.display()))?
            .to_string_lossy();
        Ok(path.with_file_name(format!("{file_name}.bak")))
    }

    fn backup_hosts_yml() -> Result<Option<PathBuf>> {
        let path = Self::get_hosts_path();
        if !path.is_file() {
            return Ok(None);
        }

        let backup_path = Self::backup_path(&path)?;
        let mut source = File::open(&path)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut backup = options.open(&backup_path)?;
        std::io::copy(&mut source, &mut backup)?;
        backup.flush()?;
        #[cfg(unix)]
        fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o600))?;
        Ok(Some(backup_path))
    }

    /// Loads hosts from the ~/.esdiag/hosts.yml (defalt) file
    pub fn parse_hosts_yml() -> Result<BTreeMap<String, KnownHost>> {
        let path = KnownHost::get_hosts_path();
        tracing::debug!("Parsing {:?}", path);
        match path.is_file() {
            true => {
                let file = File::open(path)?;
                let reader = BufReader::new(file);
                let mut hosts: BTreeMap<String, KnownHost> = serde_yaml::from_reader(reader)?;
                for (name, host) in hosts.iter_mut() {
                    host.normalize_and_validate_roles(name)?;
                }
                validate_viewer_links(&hosts)?;
                Ok(hosts)
            }
            false => {
                tracing::info!("No hosts, file creating {:?}", path);
                File::create(path)?;
                Ok(BTreeMap::new())
            }
        }
    }

    pub fn write_hosts_yml(hosts: &BTreeMap<String, KnownHost>) -> Result<String> {
        let path = KnownHost::get_hosts_path();
        let mut hosts = hosts.clone();
        for (name, host) in hosts.iter_mut() {
            host.normalize_and_validate_roles(name)?;
            host.validate_template_host_name(name)?;
            if host.secret.is_some() {
                host.clear_legacy_plaintext_auth();
            } else if host.has_legacy_plaintext_auth() {
                return Err(eyre!(
                    "Host '{name}' still contains plaintext credentials. Run `esdiag keystore migrate` first."
                ));
            }
        }
        validate_viewer_links(&hosts)?;
        tracing::debug!(
            "Writing hosts: {} to {:?}",
            hosts.clone().into_keys().collect::<Vec<String>>().join(", "),
            path
        );
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        serde_yaml::to_writer(writer, &hosts)?;
        Ok(format!("{}", &path.display()))
    }

    fn set_accept_invalid_certs(&mut self, accept_invalid_certs: bool) {
        self.accept_invalid_certs = accept_invalid_certs;
    }

    fn set_roles(&mut self, roles: Vec<HostRole>) {
        self.roles = roles;
    }

    fn normalize_and_validate_roles(&mut self, host_name: &str) -> Result<()> {
        let app = self.app().clone();
        let roles = &mut self.roles;

        if roles.is_empty() {
            roles.push(HostRole::Collect);
        }
        roles.sort_by_key(|r| match r {
            HostRole::Collect => 0,
            HostRole::Send => 1,
            HostRole::View => 2,
        });
        roles.dedup();

        for role in roles.iter() {
            match role {
                HostRole::Collect => {}
                HostRole::Send if app == Product::Elasticsearch => {}
                HostRole::View if app == Product::Kibana => {}
                HostRole::Send => {
                    return Err(eyre!(
                        "Host '{host_name}' role 'send' is only valid for Elasticsearch hosts"
                    ));
                }
                HostRole::View => {
                    return Err(eyre!("Host '{host_name}' role 'view' is only valid for Kibana hosts"));
                }
            }
        }
        Ok(())
    }
}

impl Display for KnownHost {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        let transport = self.transport_display();
        match self.auth_label() {
            "apikey" => {
                let cloud_id = self
                    .cloud_id
                    .as_ref()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| "None".to_string());
                write!(fmt, "KnownHost ApiKey: {} {} {}", self.app, transport, cloud_id)
            }
            "basic" => {
                let username = self
                    .legacy_username
                    .clone()
                    .unwrap_or_else(|| "<secret-auth>".to_string());
                write!(fmt, "KnownHost Basic: {} {}@ {}", self.app, username, transport)
            }
            "secret" => write!(fmt, "KnownHost Secret: {} {}", self.app, transport),
            _ => write!(fmt, "KnownHost NoAuth: {} {}", self.app, transport),
        }
    }
}

#[cfg(test)]
struct TestKnownHostRef<'a>(&'a KnownHost);

#[cfg(test)]
impl Serialize for TestKnownHostRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let host = self.0;
        if host.secret.is_none() && host.has_legacy_plaintext_auth() {
            if host.legacy_apikey.is_some() {
                return LegacyKnownHostRef::ApiKey {
                    accept_invalid_certs: host.accept_invalid_certs,
                    apikey: &host.legacy_apikey,
                    app: &host.app,
                    cloud_id: &host.cloud_id,
                    roles: &host.roles,
                    secret: &host.secret,
                    viewer: &host.viewer,
                    url: host
                        .concrete_url()
                        .expect("legacy test serialization requires a concrete URL"),
                }
                .serialize(serializer);
            }
            return LegacyKnownHostRef::Basic {
                accept_invalid_certs: host.accept_invalid_certs,
                app: &host.app,
                password: &host.legacy_password,
                roles: &host.roles,
                secret: &host.secret,
                viewer: &host.viewer,
                url: host
                    .concrete_url()
                    .expect("legacy test serialization requires a concrete URL"),
                username: &host.legacy_username,
            }
            .serialize(serializer);
        }

        host.serialize(serializer)
    }
}

#[cfg(test)]
struct TestKnownHostsRef<'a>(&'a BTreeMap<String, KnownHost>);

#[cfg(test)]
impl Serialize for TestKnownHostsRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, host) in self.0 {
            map.serialize_entry(name, &TestKnownHostRef(host))?;
        }
        map.end()
    }
}

#[cfg(test)]
pub(crate) fn write_hosts_yml_for_tests(hosts: &BTreeMap<String, KnownHost>) -> Result<String> {
    let path = KnownHost::get_hosts_path();
    let file = File::create(&path)?;
    let writer = BufWriter::new(file);
    serde_yaml::to_writer(writer, &TestKnownHostsRef(hosts))?;
    Ok(format!("{}", path.display()))
}

fn validate_viewer_links(hosts: &BTreeMap<String, KnownHost>) -> Result<()> {
    for (name, host) in hosts {
        if let Some(viewer_name) = host.viewer() {
            if !host.has_role(HostRole::Send) {
                return Err(eyre!(
                    "Host '{name}' sets 'viewer' but does not have required role 'send'"
                ));
            }
            let Some(viewer_host) = hosts.get(viewer_name) else {
                return Err(eyre!("Host '{name}' references unknown viewer host '{viewer_name}'"));
            };
            if !viewer_host.has_role(HostRole::View) {
                return Err(eyre!("Host '{name}' viewer '{viewer_name}' must include role 'view'"));
            }
        }
    }
    Ok(())
}

fn normalize_elastic_cloud_proxy_url(mut url: Url, cloud_id: Option<&ElasticCloud>) -> Url {
    if cloud_id.is_some() && url.path().ends_with("/proxy") {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TemplateReference {
    template_name: String,
    id: String,
    product: Option<String>,
}

fn parse_template_reference(reference: &str) -> Result<Option<TemplateReference>> {
    let Ok(url) = Url::parse(reference) else {
        return Ok(None);
    };
    if matches!(url.scheme(), "http" | "https" | "file" | "stdin" | "stdio") {
        return Ok(None);
    }
    let id = url
        .host_str()
        .ok_or_else(|| eyre!("Template reference '{reference}' is missing required `id`"))?;
    if id.trim().is_empty() {
        return Err(eyre!(
            "Template reference '{reference}' is missing required `id`"
        ));
    }
    let product_segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.trim().is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if product_segments.len() > 1 {
        return Err(eyre!(
            "Template reference '{reference}' contains too many path segments; expected at most one optional `product` segment"
        ));
    }
    let product = product_segments.into_iter().next();
    Ok(Some(TemplateReference {
        template_name: url.scheme().to_string(),
        id: id.to_string(),
        product,
    }))
}

fn validate_template_host_name(host_name: &str) -> Result<()> {
    let mut chars = host_name.chars();
    let Some(first) = chars.next() else {
        return Err(eyre!("Template host names cannot be empty"));
    };
    if !first.is_ascii_lowercase() {
        return Err(eyre!(
            "Template host '{host_name}' must start with a lowercase ASCII letter to support custom-scheme resolution"
        ));
    }
    if !chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.'))
    {
        return Err(eyre!(
            "Template host '{host_name}' must use lowercase scheme-compatible characters: [a-z][a-z0-9+.-]*"
        ));
    }
    Ok(())
}

fn validate_url_template(template: &str) -> Result<()> {
    let mut placeholders = Vec::new();
    let mut current = String::new();
    let mut in_placeholder = false;
    for ch in template.chars() {
        match ch {
            '{' if in_placeholder => {
                return Err(eyre!(
                    "Unsupported `url_template`: nested placeholders are not allowed"
                ));
            }
            '{' => {
                in_placeholder = true;
                current.clear();
            }
            '}' if !in_placeholder => {
                return Err(eyre!(
                    "Unsupported `url_template`: unmatched closing brace"
                ));
            }
            '}' => {
                in_placeholder = false;
                placeholders.push(current.clone());
                current.clear();
            }
            _ if in_placeholder => current.push(ch),
            _ => {}
        }
    }
    if in_placeholder {
        return Err(eyre!(
            "Unsupported `url_template`: unterminated placeholder"
        ));
    }
    for placeholder in placeholders {
        if !matches!(placeholder.as_str(), "id" | "product") {
            return Err(eyre!(
                "Unsupported `url_template` placeholder '{{{placeholder}}}'. Supported placeholders are `{{id}}` and `{{product}}`."
            ));
        }
    }
    render_url_template_url(template, "test-id", DEFAULT_TEMPLATE_PRODUCT)
        .map(|_| ())
        .map_err(|err| eyre!("Invalid `url_template`: {err}"))
}

fn render_url_template_url(template: &str, id: &str, product: &str) -> Result<Url> {
    let rendered = template.replace("{id}", id).replace("{product}", product);
    Url::parse(&rendered).map_err(|err| eyre!("Invalid rendered URL from template host: {err}"))
}

fn resolve_auth_with_precedence(
    secret_id: &Option<String>,
    legacy_apikey: Option<String>,
    legacy_basic: Option<(String, String)>,
) -> Result<Auth> {
    if let Some(secret_id) = secret_id {
        let auth = resolve_explicit_secret(secret_id)?;
        if legacy_apikey.is_some() || legacy_basic.is_some() {
            tracing::warn!(
                "Legacy credentials exist in hosts.yml and keystore entry '{secret_id}' exists; using keystore credentials."
            );
        }
        return Ok(auth);
    }

    if let Some(apikey) = legacy_apikey {
        return Ok(Auth::Apikey(apikey));
    }
    if let Some((username, password)) = legacy_basic {
        return Ok(Auth::Basic(username, password));
    }
    Ok(Auth::None)
}

fn resolve_explicit_secret(secret_id: &str) -> Result<Auth> {
    let keystore_password =
        get_keystore_password().map_err(|err| eyre!("Host references secret '{secret_id}' but {err}"))?;
    let secret = resolve_secret_by_id(secret_id, &keystore_password)?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    secret_auth_to_auth(secret)
}

fn secret_auth_to_auth(secret: SecretAuth) -> Result<Auth> {
    match secret {
        SecretAuth::ApiKey { apikey } => Ok(Auth::Apikey(apikey)),
        SecretAuth::Basic { username, password } => Ok(Auth::Basic(username, password)),
    }
}

impl FromStr for KnownHost {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match KnownHost::get_known(&s.to_string()) {
            Some(host) => Ok(host),
            None => Err(()),
        }
    }
}

impl From<KnownHost> for Url {
    fn from(host: KnownHost) -> Url {
        host.url
            .expect("Only concrete KnownHost values can convert into Url")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{get_secret, upsert_secret_auth, write_unlock_lease};
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    fn setup_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        let keystore = tmp.path().join("secrets.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
            std::env::set_var("ESDIAG_KEYSTORE", &keystore);
        }
        (tmp, hosts, keystore)
    }

    fn write_hosts(hosts: BTreeMap<String, KnownHost>) {
        write_hosts_yml_for_tests(&hosts).expect("write hosts");
    }

    #[test]
    fn roles_default_to_collect_when_omitted() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "default-role".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                Vec::new(),
                None,
                false,
            ),
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"default-role".to_string()).expect("host");
        assert!(host.has_role(HostRole::Collect));
    }

    #[test]
    fn invalid_send_role_on_kibana_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "kb-invalid-send".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                vec![HostRole::Send],
                None,
                false,
            ),
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected invalid role error");
        assert!(err.to_string().contains("role 'send' is only valid"));
    }

    #[test]
    fn list_by_role_filters_mixed_inventory() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "collect-only".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        hosts.insert(
            "collect-send".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9201").expect("url"),
                vec![HostRole::Collect, HostRole::Send],
                None,
                false,
            ),
        );
        hosts.insert(
            "view-host".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                vec![HostRole::View],
                None,
                false,
            ),
        );
        write_hosts(hosts);

        let collect = KnownHost::list_by_role(HostRole::Collect).expect("collect list");
        let send = KnownHost::list_by_role(HostRole::Send).expect("send list");
        let view = KnownHost::list_by_role(HostRole::View).expect("view list");

        assert_eq!(collect, vec!["collect-only".to_string(), "collect-send".to_string()]);
        assert_eq!(send, vec!["collect-send".to_string()]);
        assert_eq!(view, vec!["view-host".to_string()]);
    }

    #[test]
    fn viewer_requires_send_role_on_source_host() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                Some("viewer-host".to_string()),
                false,
            ),
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                vec![HostRole::View],
                None,
                false,
            ),
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected viewer validation error");
        assert!(err.to_string().contains("required role 'send'"));
    }

    #[test]
    fn viewer_must_reference_view_role_host() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                Some("viewer-host".to_string()),
                false,
            ),
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        let err = KnownHost::write_hosts_yml(&hosts).expect_err("expected viewer role error");
        assert!(err.to_string().contains("must include role 'view'"));
    }

    #[test]
    fn viewer_reference_valid_when_source_send_and_target_view() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "source".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Send],
                Some("viewer-host".to_string()),
                false,
            ),
        );
        hosts.insert(
            "viewer-host".to_string(),
            KnownHost::new_no_auth(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                vec![HostRole::View],
                None,
                false,
            ),
        );
        KnownHost::write_hosts_yml(&hosts).expect("viewer validation should pass");
    }

    #[test]
    fn legacy_hosts_auth_resolves_without_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy-es".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                None,
                Some("legacy-key".to_string()),
            ),
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"legacy-es".to_string()).expect("host");
        let auth = host.get_auth().expect("auth");
        assert!(matches!(auth, Auth::Apikey(k) if k == "legacy-key"));
    }

    #[test]
    fn explicit_secret_missing_keystore_fails() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
            std::env::set_var("ESDIAG_OUTPUT_APIKEY", "env-key");
            std::env::remove_var("ESDIAG_OUTPUT_USERNAME");
            std::env::remove_var("ESDIAG_OUTPUT_PASSWORD");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "secret-only".to_string(),
            KnownHost::new_legacy_basic(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                Some("missing-secret".to_string()),
                None,
            ),
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"secret-only".to_string()).expect("host");
        let err = host.get_auth().err().expect("auth should fail");
        assert!(err.to_string().contains("missing-secret"));
        unsafe {
            std::env::remove_var("ESDIAG_OUTPUT_APIKEY");
        }
    }

    #[test]
    fn explicit_secret_uses_unlock_lease_when_env_password_is_absent() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        upsert_secret_auth(
            "lease-secret",
            SecretAuth::ApiKey {
                apikey: "unlock-key".to_string(),
            },
            "pw",
        )
        .expect("upsert secret");
        write_unlock_lease("pw", std::time::Duration::from_secs(300)).expect("write unlock lease");
        unsafe {
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_legacy_basic(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                Some("lease-secret".to_string()),
                None,
            ),
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"prod-es".to_string()).expect("host");
        let auth = host.get_auth().expect("auth from unlock lease");
        assert!(matches!(auth, Auth::Apikey(key) if key == "unlock-key"));
    }

    #[test]
    fn explicit_secret_takes_precedence_over_legacy_fields() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        upsert_secret_auth(
            "custom-secret",
            SecretAuth::ApiKey {
                apikey: "secret-key".to_string(),
            },
            "pw",
        )
        .expect("upsert secret");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_legacy_basic(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                Some("custom-secret".to_string()),
                Some(("legacy-user".to_string(), "legacy-pass".to_string())),
            ),
        );
        write_hosts(hosts);

        let host = KnownHost::get_known(&"prod-es".to_string()).expect("host");
        let auth = host.get_auth().expect("auth");
        assert!(matches!(auth, Auth::Apikey(k) if k == "secret-key"));
    }

    #[test]
    fn no_secret_uses_legacy_auth_without_keystore_lookup() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        upsert_secret_auth(
            "prod-es",
            SecretAuth::ApiKey {
                apikey: "keystore-key".to_string(),
            },
            "pw",
        )
        .expect("upsert secret");

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_legacy_basic(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                None,
                Some(("legacy-user".to_string(), "legacy-pass".to_string())),
            ),
        );
        hosts.insert(
            "legacy-only".to_string(),
            KnownHost::new_legacy_basic(
                Product::Elasticsearch,
                Url::parse("http://localhost:9201").expect("url"),
                default_collect_roles(),
                None,
                false,
                None,
                Some(("legacy-only-user".to_string(), "legacy-only-pass".to_string())),
            ),
        );
        write_hosts(hosts);

        let prod_host = KnownHost::get_known(&"prod-es".to_string()).expect("host");
        let prod_auth = prod_host.get_auth().expect("auth");
        assert!(matches!(prod_auth, Auth::Basic(user, pass) if user == "legacy-user" && pass == "legacy-pass"));

        let fallback_host = KnownHost::get_known(&"legacy-only".to_string()).expect("host");
        let fallback_auth = fallback_host.get_auth().expect("auth");
        assert!(
            matches!(fallback_auth, Auth::Basic(user, pass) if user == "legacy-only-user" && pass == "legacy-only-pass")
        );
    }

    #[test]
    fn migrate_hosts_moves_legacy_credentials_to_keystore() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, _keystore) = setup_env();
        unsafe {
            std::env::set_var("ESDIAG_KEYSTORE_PASSWORD", "pw");
        }

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "es-prod".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                default_collect_roles(),
                None,
                false,
                None,
                Some("apikey-1".to_string()),
            ),
        );
        hosts.insert(
            "kb-prod".to_string(),
            KnownHost::new_legacy_basic(
                Product::Kibana,
                Url::parse("http://localhost:5601").expect("url"),
                default_collect_roles(),
                None,
                false,
                None,
                Some(("elastic".to_string(), "pass-1".to_string())),
            ),
        );
        write_hosts(hosts);

        let (migrated, unchanged) = KnownHost::migrate_hosts_to_keystore("pw").expect("migrate should succeed");
        assert_eq!(migrated, 2);
        assert_eq!(unchanged, 0);

        let migrated_hosts = KnownHost::parse_hosts_yml().expect("re-read migrated hosts");
        let raw_hosts = std::fs::read_to_string(&hosts_path).expect("read hosts file");
        let backup_path = hosts_path.with_file_name("hosts.yml.bak");
        let raw_backup_hosts = std::fs::read_to_string(&backup_path).expect("read hosts backup");

        let migrated_es = migrated_hosts.get("es-prod").expect("migrated es host");
        assert!(
            migrated_es.legacy_apikey.is_none(),
            "plaintext api key should be removed"
        );
        assert_eq!(migrated_es.secret.as_deref(), Some("es-prod"));
        let migrated_kb = migrated_hosts.get("kb-prod").expect("migrated kb host");
        assert!(
            migrated_kb.legacy_username.is_none(),
            "plaintext username should be removed"
        );
        assert!(
            migrated_kb.legacy_password.is_none(),
            "plaintext password should be removed"
        );
        assert_eq!(migrated_kb.secret.as_deref(), Some("kb-prod"));
        assert!(!raw_hosts.contains("apikey: apikey-1"));
        assert!(!raw_hosts.contains("username: elastic"));
        assert!(!raw_hosts.contains("password: pass-1"));
        assert!(raw_backup_hosts.contains("apikey: apikey-1"));
        assert!(raw_backup_hosts.contains("username: elastic"));
        assert!(raw_backup_hosts.contains("password: pass-1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&backup_path)
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "backup file should be owner-readable only");
        }

        let es_secret = get_secret("es-prod", "pw")
            .expect("get secret")
            .expect("es secret exists");
        assert_eq!(es_secret.apikey.as_deref(), Some("apikey-1"));
        assert!(es_secret.basic.is_none());

        let kb_secret = get_secret("kb-prod", "pw")
            .expect("get secret")
            .expect("kb secret exists");
        assert_eq!(kb_secret.basic.as_ref().map(|b| b.username.as_str()), Some("elastic"));
        assert_eq!(kb_secret.basic.as_ref().map(|b| b.password.as_str()), Some("pass-1"));

        let migrated_es_auth = migrated_hosts
            .get("es-prod")
            .expect("migrated es host")
            .get_auth()
            .expect("read migrated es auth");
        assert!(matches!(migrated_es_auth, Auth::Apikey(key) if key == "apikey-1"));

        let migrated_kb_auth = migrated_hosts
            .get("kb-prod")
            .expect("migrated kb host")
            .get_auth()
            .expect("read migrated kb auth");
        assert!(matches!(migrated_kb_auth, Auth::Basic(user, pass) if user == "elastic" && pass == "pass-1"));
    }

    #[test]
    fn merge_cli_update_preserves_omitted_fields() {
        let host = KnownHost::new_legacy_apikey(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![HostRole::Collect],
            None,
            true,
            None,
            Some("legacy-key".to_string()),
        );
        let update = KnownHostCliUpdate {
            roles: Some(vec![HostRole::Collect, HostRole::Send]),
            ..KnownHostCliUpdate::default()
        };

        let merged = host.merge_cli_update(&update, None).expect("merge should succeed");

        assert!(merged.accept_invalid_certs, "certificate setting should be preserved");
        assert_eq!(merged.legacy_apikey.as_deref(), Some("legacy-key"));
        assert_eq!(merged.secret, None);
        assert_eq!(merged.roles, vec![HostRole::Collect, HostRole::Send]);
    }

    #[test]
    fn merge_cli_update_switches_secret_host_to_apikey() {
        let host = KnownHost::new_legacy_basic(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![HostRole::Collect],
            None,
            false,
            Some("old-secret".to_string()),
            None,
        );
        let update = KnownHostCliUpdate {
            apikey: Some("new-key".to_string()),
            ..KnownHostCliUpdate::default()
        };

        let merged = host.merge_cli_update(&update, None).expect("merge should succeed");

        assert_eq!(merged.legacy_apikey.as_deref(), Some("new-key"));
        assert!(merged.secret.is_none(), "secret reference should be cleared");
    }

    #[test]
    fn merge_cli_update_applies_explicit_false_for_accept_invalid_certs() {
        let host = KnownHost::new_legacy_apikey(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![HostRole::Collect],
            None,
            true,
            None,
            Some("legacy-key".to_string()),
        );
        let update = KnownHostCliUpdate {
            accept_invalid_certs: Some(false),
            ..KnownHostCliUpdate::default()
        };

        let merged = host.merge_cli_update(&update, None).expect("merge should succeed");

        assert!(
            !merged.accept_invalid_certs(),
            "explicit false should clear accept_invalid_certs"
        );
    }

    #[test]
    fn write_hosts_yml_omits_false_accept_invalid_certs() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );

        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        let raw_hosts = std::fs::read_to_string(&hosts_path).expect("read hosts file");

        assert!(
            !raw_hosts.contains("accept_invalid_certs"),
            "false accept_invalid_certs should be omitted from hosts.yml"
        );
    }

    #[test]
    fn write_hosts_yml_keeps_true_accept_invalid_certs() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, hosts_path, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                true,
            ),
        );

        KnownHost::write_hosts_yml(&hosts).expect("write hosts");
        let raw_hosts = std::fs::read_to_string(&hosts_path).expect("read hosts file");

        assert!(
            raw_hosts.contains("accept_invalid_certs: true"),
            "true accept_invalid_certs should remain in hosts.yml"
        );
    }

    #[test]
    fn write_hosts_yml_rejects_plaintext_legacy_hosts() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts_path, _keystore) = setup_env();
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "legacy-es".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
                None,
                Some("legacy-key".to_string()),
            ),
        );

        let err = KnownHost::write_hosts_yml(&hosts).expect_err("legacy plaintext should be rejected");
        assert!(err.to_string().contains("Run `esdiag keystore migrate` first."));
    }

    #[test]
    fn merge_cli_update_rejects_partial_basic_auth_without_secret() {
        let host = KnownHost::new_no_auth(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![HostRole::Collect],
            None,
            false,
        );
        let update = KnownHostCliUpdate {
            username: Some("elastic".to_string()),
            ..KnownHostCliUpdate::default()
        };

        let err = match host.merge_cli_update(&update, None) {
            Ok(_) => panic!("partial basic auth should be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("either provide a secret reference or both username and password"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn merge_cli_update_rejects_partial_basic_auth_for_existing_basic_host() {
        let host = KnownHost::new_legacy_basic(
            Product::Elasticsearch,
            Url::parse("http://localhost:9200").expect("url"),
            vec![HostRole::Collect],
            None,
            false,
            None,
            Some(("old-user".to_string(), "old-pass".to_string())),
        );
        let update = KnownHostCliUpdate {
            username: Some("new-user".to_string()),
            ..KnownHostCliUpdate::default()
        };

        let err = match host.merge_cli_update(&update, None) {
            Ok(_) => panic!("partial basic auth should be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("either provide a secret reference or both username and password"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn remove_saved_deletes_existing_host_and_errors_for_missing() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "prod-es".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        write_hosts(hosts);

        KnownHost::remove_saved("prod-es").expect("remove existing host");
        let remaining = KnownHost::parse_hosts_yml().expect("re-read hosts");
        assert!(!remaining.contains_key("prod-es"));

        let err = KnownHost::remove_saved("prod-es").expect_err("missing host should error");
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn list_saved_summaries_returns_sorted_rows() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        hosts.insert(
            "z-prod".to_string(),
            KnownHost::new_no_auth(
                Product::Elasticsearch,
                Url::parse("http://localhost:9201").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
            ),
        );
        hosts.insert(
            "a-prod".to_string(),
            KnownHost::new_legacy_apikey(
                Product::Elasticsearch,
                Url::parse("http://localhost:9200").expect("url"),
                vec![HostRole::Collect],
                None,
                false,
                Some("prod-secret".to_string()),
                None,
            ),
        );
        write_hosts(hosts);

        let rows = KnownHost::list_saved_summaries().expect("summary rows");
        assert_eq!(
            rows,
            vec![
                KnownHostSummary {
                    app: "elasticsearch".to_string(),
                    name: "a-prod".to_string(),
                    secret: Some("prod-secret".to_string()),
                },
                KnownHostSummary {
                    app: "elasticsearch".to_string(),
                    name: "z-prod".to_string(),
                    secret: None,
                },
            ]
        );
    }

    #[test]
    fn template_hosts_serialize_round_trip_and_resolve_default_product() {
        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();

        let mut hosts = BTreeMap::new();
        let template_host = KnownHostBuilder::new_template(
            "https://cloud.elastic.co/api/v1/deployments/{id}/{product}".to_string(),
        )
        .build()
        .expect("build template host");
        hosts.insert("elastic-cloud".to_string(), template_host);
        write_hosts(hosts);

        let saved = KnownHost::get_known(&"elastic-cloud".to_string()).expect("saved template host");
        assert!(saved.url.is_none(), "template host should not persist a concrete URL");
        assert_eq!(
            saved.url_template(),
            Some("https://cloud.elastic.co/api/v1/deployments/{id}/{product}")
        );

        let resolved = KnownHost::resolve_template_reference("elastic-cloud://cluster-1")
            .expect("resolve template reference")
            .expect("resolved host");
        assert_eq!(resolved.app(), &Product::Elasticsearch);
        assert_eq!(
            resolved.concrete_url().map(|url| url.as_str()),
            Some("https://cloud.elastic.co/api/v1/deployments/cluster-1/elasticsearch/elasticsearch/proxy/")
        );
    }

    #[test]
    fn template_reference_rejects_extra_path_segments() {
        let err = parse_template_reference("elastic-cloud://cluster-1/elasticsearch/extra")
            .expect_err("extra path segment should fail");

        assert!(err.to_string().contains("too many path segments"));
    }

    #[test]
    fn template_host_get_url_returns_error_without_panicking() {
        let host = KnownHostBuilder::new_template("https://example.com/{id}".to_string())
            .build()
            .expect("build template host");

        let err = host.get_url().expect_err("template host has no concrete URL");
        assert!(err.to_string().contains("resolved into a concrete URL"));
    }

    #[test]
    fn template_hosts_reject_invalid_placeholders_and_non_scheme_names() {
        let err = KnownHostBuilder::new_template("https://example.com/{unsupported}".to_string())
            .build()
            .expect_err("unsupported placeholder should fail");
        assert!(err.to_string().contains("Unsupported `url_template` placeholder"));

        let _guard = env_lock().lock().expect("env lock");
        let (_tmp, _hosts, _keystore) = setup_env();
        let host = KnownHostBuilder::new_template("https://example.com/{id}".to_string())
            .build()
            .expect("valid template host");
        let err = host.save("ElasticCloud").expect_err("invalid template host name should fail");
        assert!(err.to_string().contains("lowercase ASCII letter"));
    }

    #[test]
    fn elastic_cloud_builder_rewrites_proxy_path_via_template_rendering() {
        let host = KnownHostBuilder::new(
            Url::parse("https://cloud.elastic.co/deployments/deployment-123").expect("cloud url"),
        )
        .product(Product::Elasticsearch)
        .build()
        .expect("build cloud host");
        assert_eq!(
            host.concrete_url().map(|url| url.as_str()),
            Some(
                "https://cloud.elastic.co/api/v1/deployments/deployment-123/elasticsearch/elasticsearch/proxy/"
            )
        );
    }

    #[test]
    fn elastic_cloud_admin_proxy_urls_keep_trailing_slash() {
        let host = KnownHost::from_parts(KnownHostParts {
            accept_invalid_certs: false,
            app: Product::Elasticsearch,
            cloud_id: Some(ElasticCloud::ElasticCloudAdmin),
            roles: default_collect_roles(),
            secret: None,
            viewer: None,
            url: Some(
                Url::parse("https://admin.found.no/api/v1/deployments/deployment-123/elasticsearch/main-elasticsearch/proxy")
                    .expect("cloud admin proxy url"),
            ),
            url_template: None,
            legacy_apikey: None,
            legacy_username: None,
            legacy_password: None,
        })
        .expect("build host");

        assert_eq!(
            host.concrete_url().map(|url| url.as_str()),
            Some(
                "https://admin.found.no/api/v1/deployments/deployment-123/elasticsearch/main-elasticsearch/proxy/"
            )
        );
    }
}
