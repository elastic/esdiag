// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::{
    cli_output::{CliOutcome, CommandOutcome, KeystoreOperation},
    data::{
        SecretAuth, UnlockStatus, add_secret, clear_unlock_lease, create_keystore, get_keystore_path,
        get_unlock_path, get_unlock_status, keystore_exists, remove_secret, update_secret,
        validate_existing_keystore_password, write_unlock_lease,
    },
};
use eyre::{Result, eyre};
use redact::Secret;
use std::{path::PathBuf, time::Duration};

/// A secret to store: an API key, or a username with a password.
#[derive(Debug, Clone)]
pub struct KeystoreSecretInput {
    pub secret_id: String,
    pub username: Option<String>,
    pub password: Option<Secret<String>>,
    pub apikey: Option<Secret<String>>,
    pub keystore_password: Secret<String>,
}

/// A secret to delete. When credentials are given they must match the stored secret.
#[derive(Debug, Clone)]
pub struct KeystoreRemoveInput {
    pub secret_id: String,
    pub username: Option<String>,
    pub password: Option<Secret<String>>,
    pub apikey: Option<Secret<String>>,
    pub keystore_password: Secret<String>,
}

#[derive(Debug, Clone)]
pub struct KeystoreUnlockInput {
    pub keystore_password: Secret<String>,
    pub ttl: Duration,
    /// Create the keystore with `keystore_password` when none exists.
    pub create_if_missing: bool,
}

pub fn keystore_add(input: KeystoreSecretInput) -> Result<CommandOutcome> {
    let KeystoreSecretInput {
        secret_id,
        username,
        password,
        apikey,
        keystore_password,
    } = input;
    let path = add_secret(
        &secret_id,
        username,
        password.map(expose),
        apikey.map(expose),
        keystore_password.expose_secret(),
    )?;
    tracing::info!("Secret '{secret_id}' saved to {path}");
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreChanged {
        operation: KeystoreOperation::Added,
        secret_id: Some(secret_id),
        path: Some(path),
    }))
}

pub fn keystore_update(input: KeystoreSecretInput) -> Result<CommandOutcome> {
    let KeystoreSecretInput {
        secret_id,
        username,
        password,
        apikey,
        keystore_password,
    } = input;
    let path = update_secret(
        &secret_id,
        username,
        password.map(expose),
        apikey.map(expose),
        keystore_password.expose_secret(),
    )?;
    tracing::info!("Secret '{secret_id}' updated in {path}");
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreChanged {
        operation: KeystoreOperation::Updated,
        secret_id: Some(secret_id),
        path: Some(path),
    }))
}

pub fn keystore_remove(input: KeystoreRemoveInput) -> Result<CommandOutcome> {
    let KeystoreRemoveInput {
        secret_id,
        username,
        password,
        apikey,
        keystore_password,
    } = input;
    let expected = expected_secret_auth(username, password.map(expose), apikey.map(expose))?;
    let path = remove_secret(&secret_id, expected, keystore_password.expose_secret())?;
    tracing::info!("Secret '{secret_id}' deleted from {path}");
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreChanged {
        operation: KeystoreOperation::Removed,
        secret_id: Some(secret_id),
        path: Some(path),
    }))
}

pub fn keystore_unlock(input: KeystoreUnlockInput) -> Result<CommandOutcome> {
    let unlock_path = unlock_lease(&input.keystore_password, input.ttl, input.create_if_missing)?;
    let status = get_unlock_status()?;
    if let Some(expires_at_epoch) = status.expires_at_epoch {
        tracing::info!(
            "Keystore unlocked via {} until {} ({})",
            unlock_path.display(),
            format_epoch(expires_at_epoch),
            format_remaining_duration(expires_at_epoch)
        );
    } else {
        tracing::info!("Keystore unlocked via {}", unlock_path.display());
    }
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreStatus {
        exists: status.keystore_exists,
        unlock_active: status.unlock_active,
        expires_at_epoch: status.expires_at_epoch,
    }))
}

/// Writes an unlock lease after validating `keystore_password`, creating the
/// keystore first when it is missing and `create_if_missing` is set.
pub fn unlock_lease(keystore_password: &Secret<String>, ttl: Duration, create_if_missing: bool) -> Result<PathBuf> {
    if keystore_exists()? {
        validate_existing_keystore_password(keystore_password.expose_secret())?;
    } else if create_if_missing {
        create_keystore(keystore_password.expose_secret())?;
    } else {
        return Err(eyre!("No keystore exists at {}", get_keystore_path()?.display()));
    }
    write_unlock_lease(keystore_password.expose_secret(), ttl)
}

pub fn keystore_lock() -> Result<CommandOutcome> {
    let status = UnlockStatus {
        keystore_exists: keystore_exists()?,
        unlock_active: false,
        expires_at_epoch: None,
        unlock_path: get_unlock_path()?,
    };
    if clear_unlock_lease()? {
        tracing::info!("Keystore unlock lease removed");
    } else {
        tracing::info!("Keystore unlock lease was already absent");
    }
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreStatus {
        exists: status.keystore_exists,
        unlock_active: false,
        expires_at_epoch: None,
    }))
}

pub fn keystore_status() -> Result<CommandOutcome> {
    let status = get_unlock_status()?;
    let keystore_path = get_keystore_path()?;
    tracing::info!(
        "Keystore: {} ({})",
        if status.keystore_exists { "present" } else { "absent" },
        keystore_path.display()
    );
    Ok(CommandOutcome::Completed(CliOutcome::KeystoreStatus {
        exists: status.keystore_exists,
        unlock_active: status.unlock_active,
        expires_at_epoch: status.expires_at_epoch,
    }))
}

fn expose(secret: Secret<String>) -> String {
    secret.expose_secret().clone()
}

fn expected_secret_auth(
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
) -> Result<Option<SecretAuth>> {
    match (apikey, username, password) {
        (None, None, None) => Ok(None),
        (Some(apikey), None, None) => Ok(Some(SecretAuth::apikey(apikey))),
        (None, Some(username), Some(password)) => Ok(Some(SecretAuth::basic(username, password))),
        _ => Err(eyre!(
            "Invalid auth options: use either --apikey or --user with --password"
        )),
    }
}

fn format_epoch(epoch_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(epoch_seconds, 0)
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| epoch_seconds.to_string())
}

fn format_remaining_duration(expires_at_epoch: i64) -> String {
    format_remaining_duration_from(chrono::Utc::now().timestamp(), expires_at_epoch)
}

fn format_remaining_duration_from(now_epoch: i64, expires_at_epoch: i64) -> String {
    let remaining = expires_at_epoch.saturating_sub(now_epoch);
    let duration = Duration::from_secs(remaining.max(0) as u64);
    let days = duration.as_secs() / 86_400;
    let hours = (duration.as_secs() % 86_400) / 3_600;
    let minutes = (duration.as_secs() % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h remaining")
    } else if hours > 0 {
        format!("{hours}h {minutes}m remaining")
    } else {
        format!("{minutes}m remaining")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_duration_formats_hours_and_minutes() {
        assert_eq!(
            format_remaining_duration_from(1_700_000_000, 1_700_003_660),
            "1h 1m remaining"
        );
    }
}
