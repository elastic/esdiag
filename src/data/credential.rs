// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::keystore::{get_keystore_password, resolve_secret_auth};
use super::{Auth, SecretAuth};
use eyre::{Result, eyre};

/// Where a host's credential comes from.
///
/// Only a keystore reference is written to `hosts.yml` (ADR-0011). Runtime
/// credentials arrive from the environment, an Elastic CLI context, or an
/// ad-hoc request, and a host carrying one cannot be saved. Legacy credentials
/// are plaintext read from a pre-migration `hosts.yml`; they keep resolving
/// until `esdiag keystore migrate` moves them into the keystore.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Credential {
    #[default]
    None,
    Keystore(String),
    Runtime(SecretAuth),
    Legacy {
        auth: SecretAuth,
        secret: Option<String>,
    },
}

impl Credential {
    /// Wrap credentials supplied at run time; [`Auth::None`] is no credential.
    pub fn runtime(auth: Auth) -> Self {
        match auth {
            Auth::Apikey(apikey) => Self::Runtime(SecretAuth::ApiKey { apikey }),
            Auth::Basic(username, password) => Self::Runtime(SecretAuth::Basic { username, password }),
            Auth::None => Self::None,
        }
    }

    pub fn keystore(secret: Option<String>) -> Self {
        secret.map_or(Self::None, Self::Keystore)
    }

    /// Classify the credential fields read from a `hosts.yml` entry.
    pub fn from_hosts_yml(secret: Option<String>, auth: Auth) -> Self {
        match Self::runtime(auth) {
            Self::Runtime(auth) => Self::Legacy { auth, secret },
            _ => Self::keystore(secret),
        }
    }

    pub fn secret_reference(&self) -> Option<&str> {
        match self {
            Self::Keystore(secret)
            | Self::Legacy {
                secret: Some(secret), ..
            } => Some(secret),
            _ => None,
        }
    }

    /// Pre-migration plaintext that `esdiag keystore migrate` should move.
    pub fn legacy_auth(&self) -> Option<&SecretAuth> {
        match self {
            Self::Legacy { auth, .. } => Some(auth),
            _ => None,
        }
    }

    /// The credential `hosts.yml` may hold for this host, or an error naming
    /// why plaintext would otherwise be written.
    pub(crate) fn persisted(&self) -> Result<Self, PersistError> {
        match self {
            Self::None | Self::Keystore(_) => Ok(self.clone()),
            Self::Legacy {
                secret: Some(secret), ..
            } => Ok(Self::Keystore(secret.clone())),
            Self::Legacy { secret: None, .. } => Err(PersistError::Legacy),
            Self::Runtime(_) => Err(PersistError::Runtime),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            _ if self.secret_reference().is_some() => "secret",
            Self::Runtime(SecretAuth::ApiKey { .. })
            | Self::Legacy {
                auth: SecretAuth::ApiKey { .. },
                ..
            } => "apikey",
            Self::Runtime(SecretAuth::Basic { .. })
            | Self::Legacy {
                auth: SecretAuth::Basic { .. },
                ..
            } => "basic",
            _ => "none",
        }
    }

    pub(crate) fn basic_username(&self) -> Option<&str> {
        match self {
            Self::Runtime(SecretAuth::Basic { username, .. })
            | Self::Legacy {
                auth: SecretAuth::Basic { username, .. },
                ..
            } => Some(username),
            _ => None,
        }
    }

    /// Produce the credential material, unlocking the keystore when needed.
    pub fn resolve(&self) -> Result<Auth> {
        match self {
            Self::None => Ok(Auth::None),
            Self::Keystore(secret) => resolve_keystore_secret(secret),
            Self::Runtime(auth) => Ok(auth.clone().into()),
            Self::Legacy {
                secret: Some(secret), ..
            } => {
                let auth = resolve_keystore_secret(secret)?;
                tracing::warn!(
                    "Legacy credentials exist in hosts.yml and keystore entry '{secret}' exists; using keystore credentials."
                );
                Ok(auth)
            }
            Self::Legacy { auth, secret: None } => Ok(auth.clone().into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistError {
    Legacy,
    Runtime,
}

impl From<SecretAuth> for Auth {
    fn from(secret: SecretAuth) -> Self {
        match secret {
            SecretAuth::ApiKey { apikey } => Auth::Apikey(apikey),
            SecretAuth::Basic { username, password } => Auth::Basic(username, password),
        }
    }
}

fn resolve_keystore_secret(secret_id: &str) -> Result<Auth> {
    let keystore_password =
        get_keystore_password().map_err(|err| eyre!("Host references secret '{secret_id}' but {err}"))?;
    let secret = resolve_secret_auth(secret_id, keystore_password.expose_secret())?
        .ok_or_else(|| eyre!("Secret '{secret_id}' was not found in keystore"))?;
    Ok(secret.into())
}

#[cfg(test)]
mod tests {
    use super::{Credential, PersistError};
    use crate::data::{Auth, SecretAuth};

    #[test]
    fn runtime_without_auth_is_no_credential() {
        assert_eq!(Credential::runtime(Auth::None), Credential::None);
    }

    #[test]
    fn runtime_credentials_cannot_be_persisted() {
        let credential = Credential::runtime(Auth::apikey("context-key"));

        assert_eq!(credential.persisted(), Err(PersistError::Runtime));
        assert!(credential.legacy_auth().is_none(), "runtime is not migration input");
        assert_eq!(credential.resolve().expect("resolve"), Auth::apikey("context-key"));
    }

    #[test]
    fn hosts_yml_plaintext_is_legacy_and_persists_only_its_secret() {
        let plaintext = Credential::from_hosts_yml(None, Auth::basic("elastic", "pw"));
        let referenced = Credential::from_hosts_yml(Some("prod".to_string()), Auth::apikey("old"));

        assert_eq!(plaintext.legacy_auth(), Some(&SecretAuth::basic("elastic", "pw")));
        assert_eq!(plaintext.persisted(), Err(PersistError::Legacy));
        assert_eq!(referenced.persisted(), Ok(Credential::Keystore("prod".to_string())));
        assert_eq!(referenced.label(), "secret");
    }

    #[test]
    fn hosts_yml_without_plaintext_is_a_keystore_reference() {
        assert_eq!(
            Credential::from_hosts_yml(Some("prod".to_string()), Auth::None),
            Credential::Keystore("prod".to_string())
        );
        assert_eq!(Credential::from_hosts_yml(None, Auth::None), Credential::None);
    }
}
