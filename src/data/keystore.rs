// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use crate::env::ESDIAG_KEYSTORE_PASSWORD;
use aes_gcm_siv::{
    Aes256GcmSiv, Nonce,
    aead::{Aead, KeyInit},
};
use base64::Engine;
use eyre::{Result, eyre};
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    env,
    fs::{File, OpenOptions},
    future::Future,
    io::{BufReader, BufWriter, IsTerminal, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const KDF_ROUNDS: u32 = 100_000;
const KEY_SIZE: usize = 32;
const SALT_SIZE: usize = 16;
const NONCE_SIZE: usize = 12;
const KEYSTORE_FILE: &str = "secrets.yml";
const UNLOCK_FILE: &str = "keystore.unlock";
const DEFAULT_UNLOCK_TTL_SECS: u64 = 24 * 60 * 60;
const MAX_UNLOCK_TTL_SECS: u64 = 30 * 24 * 60 * 60;
const MINIMAL_UNLOCK_CONTEXT: &str = "esdiag-keystore-unlock";

tokio::task_local! {
    static SCOPED_KEYSTORE_PASSWORD: String;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SecretAuth {
    ApiKey { apikey: String },
    Basic { username: String, password: String },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BasicSecret {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SecretEntry {
    #[serde(rename = "ApiKey", skip_serializing_if = "Option::is_none")]
    pub apikey: Option<String>,
    #[serde(rename = "Basic", skip_serializing_if = "Option::is_none")]
    pub basic: Option<BasicSecret>,
}

impl SecretEntry {
    pub fn is_empty(&self) -> bool {
        self.apikey.is_none() && self.basic.is_none()
    }

    pub fn upsert_auth(&mut self, auth: SecretAuth) {
        match auth {
            SecretAuth::ApiKey { apikey } => self.apikey = Some(apikey),
            SecretAuth::Basic { username, password } => {
                self.basic = Some(BasicSecret { username, password });
            }
        }
    }

    pub fn remove_auth(&mut self, auth: &SecretAuth) {
        match auth {
            SecretAuth::ApiKey { .. } => self.apikey = None,
            SecretAuth::Basic { .. } => self.basic = None,
        }
    }

    pub fn resolve_auth(&self) -> Option<SecretAuth> {
        if let Some(apikey) = &self.apikey {
            return Some(SecretAuth::ApiKey {
                apikey: apikey.clone(),
            });
        }
        self.basic.as_ref().map(|basic| SecretAuth::Basic {
            username: basic.username.clone(),
            password: basic.password.clone(),
        })
    }

    pub fn contains_auth(&self, auth: &SecretAuth) -> bool {
        match auth {
            SecretAuth::ApiKey { apikey } => self.apikey.as_ref() == Some(apikey),
            SecretAuth::Basic { username, password } => self
                .basic
                .as_ref()
                .is_some_and(|b| b.username == *username && b.password == *password),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct KeystoreData {
    version: u8,
    secrets: BTreeMap<String, SecretEntry>,
}

impl Default for KeystoreData {
    fn default() -> Self {
        Self {
            version: 1,
            secrets: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncryptedKeystore {
    version: u8,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UnlockLeaseData {
    version: u8,
    expires_at_epoch: i64,
    password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncryptedUnlockLease {
    version: u8,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlockStatus {
    pub keystore_exists: bool,
    pub unlock_active: bool,
    pub expires_at_epoch: Option<i64>,
    pub unlock_path: PathBuf,
}

#[derive(Clone, Debug)]
struct UnlockLease {
    expires_at_epoch: i64,
    password: String,
}

fn current_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64
}

fn get_home_dir() -> Result<String> {
    match std::env::consts::OS {
        "windows" => env::var("USERPROFILE").map_err(Into::into),
        "linux" | "macos" => env::var("HOME").map_err(Into::into),
        os => Err(eyre!("Unknown home directory for operating system: {os}")),
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| eyre!("Path '{}' has no parent directory", path.display()))?;
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn secure_output_file(path: &Path) -> Result<File> {
    ensure_parent_dir(path)?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn temp_output_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| eyre!("Path '{}' has no parent directory", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| eyre!("Path '{}' has no file name", path.display()))?
        .to_string_lossy();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    Ok(parent.join(format!(
        ".{file_name}.tmp-{}-{unique}",
        std::process::id()
    )))
}

fn replace_file_atomic(path: &Path, temp_path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    std::fs::rename(temp_path, path)?;
    Ok(())
}

fn write_yaml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temp_path = temp_output_path(path)?;
    let write_result = (|| -> Result<()> {
        let file = secure_output_file(&temp_path)?;
        let mut writer = BufWriter::new(file);
        serde_yaml::to_writer(&mut writer, value)?;
        writer.flush()?;
        drop(writer);
        replace_file_atomic(path, &temp_path)
    })();
    if write_result.is_err() && temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

pub fn get_keystore_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("ESDIAG_KEYSTORE") {
        let path = PathBuf::from(path);
        ensure_parent_dir(&path)?;
        return Ok(path);
    }
    let home_dir = get_home_dir()?;
    let esdiag_dir = PathBuf::from(home_dir).join(".esdiag");
    if !esdiag_dir.exists() {
        std::fs::create_dir_all(&esdiag_dir)?;
    }
    Ok(esdiag_dir.join(KEYSTORE_FILE))
}

pub fn get_unlock_path() -> Result<PathBuf> {
    let keystore_path = get_keystore_path()?;
    let parent = keystore_path
        .parent()
        .ok_or_else(|| eyre!("Keystore path '{}' has no parent directory", keystore_path.display()))?;
    Ok(parent.join(UNLOCK_FILE))
}

pub fn default_unlock_ttl() -> Duration {
    Duration::from_secs(DEFAULT_UNLOCK_TTL_SECS)
}

pub fn parse_unlock_ttl(input: &str) -> Result<Duration> {
    if input.len() < 2 {
        return Err(eyre!(
            "Invalid unlock TTL '{input}'. Use an integer followed by m, h, or d."
        ));
    }
    let (value, suffix) = input.split_at(input.len() - 1);
    let value: u64 = value.parse().map_err(|_| {
        eyre!("Invalid unlock TTL '{input}'. Use an integer followed by m, h, or d.")
    })?;
    let seconds = match suffix {
        "m" => value.saturating_mul(60),
        "h" => value.saturating_mul(60 * 60),
        "d" => value.saturating_mul(24 * 60 * 60),
        _ => {
            return Err(eyre!(
                "Invalid unlock TTL '{input}'. Use an integer followed by m, h, or d."
            ));
        }
    };
    if seconds == 0 {
        return Err(eyre!("Unlock TTL must be greater than zero."));
    }
    if seconds > MAX_UNLOCK_TTL_SECS {
        return Err(eyre!("Unlock TTL may not exceed 30d."));
    }
    Ok(Duration::from_secs(seconds))
}

fn parse_secret_auth(
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
) -> Result<SecretAuth> {
    match (apikey, username, password) {
        (Some(apikey), None, None) => Ok(SecretAuth::ApiKey { apikey }),
        (None, Some(username), Some(password)) => Ok(SecretAuth::Basic { username, password }),
        _ => Err(eyre!(
            "Invalid secret auth: use either --apikey or --user with --password"
        )),
    }
}

fn unlock_context_material() -> Result<String> {
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-user".to_string());
    let home = get_home_dir()?;
    let keystore = get_keystore_path()?;
    let host = env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string());
    Ok(format!(
        "{MINIMAL_UNLOCK_CONTEXT}:{}:{user}:{home}:{host}:{}",
        std::env::consts::OS,
        keystore.display()
    ))
}

fn derive_context_key(context: &str, salt: &[u8]) -> [u8; KEY_SIZE] {
    let mut key = [0_u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(context.as_bytes(), salt, KDF_ROUNDS, &mut key);
    key
}

fn encrypt_unlock_lease(data: &UnlockLeaseData) -> Result<EncryptedUnlockLease> {
    let salt = rand::random::<[u8; SALT_SIZE]>();
    let nonce = rand::random::<[u8; NONCE_SIZE]>();
    let key = derive_context_key(&unlock_context_material()?, &salt);
    let plaintext = serde_yaml::to_string(data)?;
    let cipher = Aes256GcmSiv::new_from_slice(&key)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| eyre!("Failed to encrypt unlock lease"))?;
    Ok(EncryptedUnlockLease {
        version: 1,
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

fn decrypt_unlock_lease(encrypted: EncryptedUnlockLease) -> Result<UnlockLeaseData> {
    if encrypted.version != 1 {
        return Err(eyre!(
            "Unsupported unlock lease version {}",
            encrypted.version
        ));
    }
    let salt = base64::engine::general_purpose::STANDARD.decode(encrypted.salt)?;
    let nonce = base64::engine::general_purpose::STANDARD.decode(encrypted.nonce)?;
    let ciphertext = base64::engine::general_purpose::STANDARD.decode(encrypted.ciphertext)?;
    if salt.len() != SALT_SIZE {
        return Err(eyre!(
            "Invalid unlock lease salt length {}, expected {}",
            salt.len(),
            SALT_SIZE
        ));
    }
    if nonce.len() != NONCE_SIZE {
        return Err(eyre!(
            "Invalid unlock lease nonce length {}, expected {}",
            nonce.len(),
            NONCE_SIZE
        ));
    }
    let key = derive_context_key(&unlock_context_material()?, &salt);
    let cipher = Aes256GcmSiv::new_from_slice(&key)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| eyre!("Failed to decrypt unlock lease"))?;
    Ok(serde_yaml::from_slice(&plaintext)?)
}

fn remove_unlock_file_best_effort(path: &Path, reason: &str) {
    if !path.exists() {
        return;
    }
    if let Err(err) = std::fs::remove_file(path) {
        tracing::warn!(
            "Failed to delete {} unlock lease '{}': {}",
            reason,
            path.display(),
            err
        );
    }
}

fn read_unlock_lease() -> Result<Option<UnlockLease>> {
    let path = get_unlock_path()?;
    if !path.is_file() {
        return Ok(None);
    }
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(err) => {
            tracing::warn!("Failed to read unlock lease '{}': {}", path.display(), err);
            return Ok(None);
        }
    };
    let reader = BufReader::new(file);
    let encrypted: EncryptedUnlockLease = match serde_yaml::from_reader(reader) {
        Ok(data) => data,
        Err(err) => {
            tracing::warn!("Invalid unlock lease '{}': {}", path.display(), err);
            return Ok(None);
        }
    };
    let lease = match decrypt_unlock_lease(encrypted) {
        Ok(lease) => lease,
        Err(err) => {
            tracing::warn!("Invalid unlock lease '{}': {}", path.display(), err);
            return Ok(None);
        }
    };
    if lease.version != 1 {
        tracing::warn!(
            "Unsupported unlock lease version {} in '{}'",
            lease.version,
            path.display()
        );
        return Ok(None);
    }
    if lease.expires_at_epoch <= current_epoch_seconds() {
        remove_unlock_file_best_effort(&path, "expired");
        return Ok(None);
    }
    Ok(Some(UnlockLease {
        expires_at_epoch: lease.expires_at_epoch,
        password: lease.password,
    }))
}

fn write_unlock_lease_until(keystore_password: &str, expires_at_epoch: i64) -> Result<PathBuf> {
    let data = UnlockLeaseData {
        version: 1,
        expires_at_epoch,
        password: keystore_password.to_string(),
    };
    let envelope = encrypt_unlock_lease(&data)?;
    let path = get_unlock_path()?;
    write_yaml_atomic(&path, &envelope)?;
    Ok(path)
}

pub fn write_unlock_lease(keystore_password: &str, ttl: Duration) -> Result<PathBuf> {
    let expires_at_epoch = current_epoch_seconds() + ttl.as_secs() as i64;
    write_unlock_lease_until(keystore_password, expires_at_epoch)
}

pub fn clear_unlock_lease() -> Result<bool> {
    let path = get_unlock_path()?;
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

pub fn get_unlock_status() -> Result<UnlockStatus> {
    let unlock_path = get_unlock_path()?;
    let lease = read_unlock_lease()?;
    Ok(UnlockStatus {
        keystore_exists: get_keystore_path()?.is_file(),
        unlock_active: lease.is_some(),
        expires_at_epoch: lease.as_ref().map(|lease| lease.expires_at_epoch),
        unlock_path,
    })
}

pub fn get_keystore_password() -> Result<String> {
    if let Ok(password) = SCOPED_KEYSTORE_PASSWORD.try_with(Clone::clone) {
        return Ok(password);
    }
    if let Ok(password) = env::var(ESDIAG_KEYSTORE_PASSWORD) {
        return Ok(password);
    }
    if let Some(lease) = read_unlock_lease()? {
        return Ok(lease.password);
    }
    Err(eyre!(
        "{ESDIAG_KEYSTORE_PASSWORD} is not set and no valid unlock lease is available; cannot decrypt secrets from keystore."
    ))
}

pub fn get_password_from_unlock_file() -> Result<Option<String>> {
    Ok(read_unlock_lease()?.map(|lease| lease.password))
}

pub async fn with_scoped_keystore_password<F>(keystore_password: String, future: F) -> F::Output
where
    F: Future,
{
    SCOPED_KEYSTORE_PASSWORD
        .scope(keystore_password, future)
        .await
}

fn get_password_for_secret_commands_with_prompt<F>(interactive: bool, prompt_fn: F) -> Result<String>
where
    F: FnOnce(&str) -> Result<String>,
{
    if let Ok(password) = env::var(ESDIAG_KEYSTORE_PASSWORD) {
        return Ok(password);
    }
    if let Some(lease) = read_unlock_lease()? {
        if !keystore_exists()? || validate_existing_keystore_password(&lease.password).is_ok() {
            return Ok(lease.password);
        }
        let unlock_path = get_unlock_path()?;
        remove_unlock_file_best_effort(&unlock_path, "stale");
    }

    if interactive {
        let prompt = format!("Enter keystore password ({ESDIAG_KEYSTORE_PASSWORD}): ");
        return prompt_fn(&prompt);
    }

    Err(eyre!(
        "{ESDIAG_KEYSTORE_PASSWORD} is not set and no interactive terminal is available."
    ))
}

pub fn get_password_for_secret_commands() -> Result<String> {
    get_password_for_secret_commands_with_prompt(
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        |prompt| Ok(rpassword::prompt_password(prompt)?),
    )
}

pub fn add_secret(
    secret_id: &str,
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
    keystore_password: &str,
) -> Result<String> {
    let auth = parse_secret_auth(username, password, apikey)?;
    let mut store = read_store(keystore_password)?;
    if store.secrets.contains_key(secret_id) {
        return Err(eyre!("Secret '{secret_id}' already exists"));
    }
    store
        .secrets
        .entry(secret_id.to_string())
        .or_insert_with(SecretEntry::default)
        .upsert_auth(auth);
    write_store(&store, keystore_password)?;
    Ok(get_keystore_path()?.display().to_string())
}

pub fn update_secret(
    secret_id: &str,
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
    keystore_password: &str,
) -> Result<String> {
    let auth = parse_secret_auth(username, password, apikey)?;
    let mut store = read_store(keystore_password)?;
    let entry = store
        .secrets
        .get_mut(secret_id)
        .ok_or_else(|| eyre!("Secret '{secret_id}' not found"))?;
    *entry = SecretEntry::default();
    entry.upsert_auth(auth);
    write_store(&store, keystore_password)?;
    Ok(get_keystore_path()?.display().to_string())
}

pub fn remove_secret(
    secret_id: &str,
    expected: Option<SecretAuth>,
    keystore_password: &str,
) -> Result<String> {
    let mut store = read_store(keystore_password)?;
    let entry = store
        .secrets
        .get_mut(secret_id)
        .ok_or_else(|| eyre!("Secret '{secret_id}' not found"))?;

    if let Some(expected) = expected {
        if !entry.contains_auth(&expected) {
            return Err(eyre!(
                "Provided auth options do not match the stored secret for '{secret_id}'"
            ));
        }
        entry.remove_auth(&expected);
        if entry.is_empty() {
            store.secrets.remove(secret_id);
        }
    } else {
        store.secrets.remove(secret_id);
    }

    write_store(&store, keystore_password)?;
    Ok(get_keystore_path()?.display().to_string())
}

pub fn get_secret(secret_id: &str, keystore_password: &str) -> Result<Option<SecretEntry>> {
    let store = read_store(keystore_password)?;
    Ok(store.secrets.get(secret_id).cloned())
}

pub fn upsert_secret_auth(
    secret_id: &str,
    auth: SecretAuth,
    keystore_password: &str,
) -> Result<()> {
    let mut store = read_store(keystore_password)?;
    let entry = store
        .secrets
        .entry(secret_id.to_string())
        .or_insert_with(SecretEntry::default);
    entry.upsert_auth(auth);
    write_store(&store, keystore_password)?;
    Ok(())
}

pub(crate) fn upsert_secret_auth_batch<I>(entries: I, keystore_password: &str) -> Result<usize>
where
    I: IntoIterator<Item = (String, SecretAuth)>,
{
    let mut store = read_store(keystore_password)?;
    let mut updated = 0_usize;

    for (secret_id, auth) in entries {
        let entry = store
            .secrets
            .entry(secret_id)
            .or_insert_with(SecretEntry::default);
        entry.upsert_auth(auth);
        updated += 1;
    }

    if updated > 0 {
        write_store(&store, keystore_password)?;
    }
    Ok(updated)
}

pub fn resolve_secret_auth(secret_id: &str, keystore_password: &str) -> Result<Option<SecretAuth>> {
    let store = read_store(keystore_password)?;
    Ok(store
        .secrets
        .get(secret_id)
        .and_then(SecretEntry::resolve_auth))
}

pub fn authenticate(keystore_password: &str) -> Result<()> {
    if keystore_exists()? {
        validate_existing_keystore_password(keystore_password)
    } else {
        create_keystore(keystore_password).map(|_| ())
    }
}

pub fn validate_existing_keystore_password(keystore_password: &str) -> Result<()> {
    if !keystore_exists()? {
        return Err(eyre!(
            "No keystore exists at {}",
            get_keystore_path()?.display()
        ));
    }
    read_store(keystore_password).map(|_| ())
}

pub fn create_keystore(keystore_password: &str) -> Result<String> {
    let path = get_keystore_path()?;
    if path.is_file() {
        return Err(eyre!("Keystore already exists at {}", path.display()));
    }
    let store = KeystoreData::default();
    write_store(&store, keystore_password)?;
    tracing::info!("Created empty keystore at {}", path.display());
    Ok(path.display().to_string())
}

pub fn rotate_keystore_password(current_password: &str, new_password: &str) -> Result<String> {
    let store = read_store(current_password)?;
    write_store(&store, new_password)?;
    if let Some(lease) = read_unlock_lease()? {
        write_unlock_lease_until(new_password, lease.expires_at_epoch)?;
    }
    Ok(get_keystore_path()?.display().to_string())
}

pub fn list_secret_names(keystore_password: &str) -> Result<Vec<String>> {
    let store = read_store(keystore_password)?;
    let mut names: Vec<String> = store.secrets.keys().cloned().collect();
    names.sort();
    Ok(names)
}

pub fn keystore_exists() -> Result<bool> {
    Ok(get_keystore_path()?.is_file())
}

fn read_store(keystore_password: &str) -> Result<KeystoreData> {
    let path = get_keystore_path()?;
    if !path.is_file() {
        return Ok(KeystoreData::default());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let encrypted: EncryptedKeystore = serde_yaml::from_reader(reader)?;
    if encrypted.version != 1 {
        return Err(eyre!("Unsupported keystore version {}", encrypted.version));
    }

    let salt = base64::engine::general_purpose::STANDARD.decode(encrypted.salt)?;
    let nonce = base64::engine::general_purpose::STANDARD.decode(encrypted.nonce)?;
    let ciphertext = base64::engine::general_purpose::STANDARD.decode(encrypted.ciphertext)?;
    if salt.len() != SALT_SIZE {
        return Err(eyre!(
            "Invalid keystore salt length {}, expected {}",
            salt.len(),
            SALT_SIZE
        ));
    }
    if nonce.len() != NONCE_SIZE {
        return Err(eyre!(
            "Invalid keystore nonce length {}, expected {}",
            nonce.len(),
            NONCE_SIZE
        ));
    }

    let key = derive_key(keystore_password, &salt);
    let cipher = Aes256GcmSiv::new_from_slice(&key)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| eyre!("Failed to decrypt keystore. Check keystore password."))?;
    Ok(serde_yaml::from_slice(&plaintext)?)
}

fn write_store(store: &KeystoreData, keystore_password: &str) -> Result<()> {
    let salt = rand::random::<[u8; SALT_SIZE]>();
    let nonce = rand::random::<[u8; NONCE_SIZE]>();
    let key = derive_key(keystore_password, &salt);
    let plaintext = serde_yaml::to_string(store)?;

    let cipher = Aes256GcmSiv::new_from_slice(&key)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| eyre!("Failed to encrypt keystore"))?;

    let envelope = EncryptedKeystore {
        version: 1,
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    };

    let path = get_keystore_path()?;
    write_yaml_atomic(&path, &envelope)?;
    Ok(())
}

fn derive_key(password: &str, salt: &[u8]) -> [u8; KEY_SIZE] {
    let mut key = [0_u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, KDF_ROUNDS, &mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock;
    use tempfile::TempDir;

    fn setup_env() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let keystore_path = config_dir.join("secrets.yml");
        let unlock_path = config_dir.join("keystore.unlock");
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("USERPROFILE", tmp.path());
            std::env::set_var("ESDIAG_KEYSTORE", &keystore_path);
            std::env::remove_var("ESDIAG_KEYSTORE_PASSWORD");
        }
        (tmp, keystore_path, unlock_path)
    }

    #[test]
    fn unlock_ttl_parses_supported_suffixes() {
        assert_eq!(parse_unlock_ttl("90m").expect("parse").as_secs(), 5_400);
        assert_eq!(parse_unlock_ttl("24h").expect("parse").as_secs(), 86_400);
        assert_eq!(parse_unlock_ttl("7d").expect("parse").as_secs(), 604_800);
    }

    #[test]
    fn unlock_ttl_rejects_values_above_thirty_days() {
        assert!(parse_unlock_ttl("31d").is_err());
    }

    #[test]
    fn expired_unlock_lease_is_removed_and_not_used() {
        let _guard = test_env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path, unlock_path) = setup_env();

        create_keystore("pw").expect("create keystore");
        write_unlock_lease_until("pw", current_epoch_seconds() - 1).expect("write expired lease");

        let status = get_unlock_status().expect("status");
        assert!(!status.unlock_active);
        assert!(!unlock_path.exists(), "expired unlock file should be deleted");
        assert!(
            get_keystore_password().is_err(),
            "expired lease should not provide password"
        );
    }

    #[test]
    fn rotate_keystore_password_preserves_secret_and_unlock_lease() {
        let _guard = test_env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path, _unlock_path) = setup_env();

        create_keystore("pw").expect("create keystore");
        add_secret(
            "api-secret",
            None,
            None,
            Some("secret-key".to_string()),
            "pw",
        )
        .expect("add secret");
        write_unlock_lease_until("pw", current_epoch_seconds() + 300).expect("write unlock lease");

        rotate_keystore_password("pw", "new-pw").expect("rotate password");

        let secret = get_secret("api-secret", "new-pw")
            .expect("read secret")
            .expect("secret exists");
        assert_eq!(secret.apikey.as_deref(), Some("secret-key"));
        assert_eq!(
            get_keystore_password().expect("unlock lease password"),
            "new-pw"
        );
    }

    #[test]
    fn clear_unlock_lease_treats_non_file_paths_as_absent() {
        let _guard = test_env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path, unlock_path) = setup_env();
        std::fs::create_dir_all(&unlock_path).expect("create unlock directory");

        assert!(!clear_unlock_lease().expect("clear unlock lease"));
        assert!(unlock_path.is_dir(), "non-file unlock path should be left alone");
    }

    #[test]
    fn stale_unlock_lease_for_secret_commands_falls_back_to_prompt() {
        let _guard = test_env_lock().lock().expect("env lock");
        let (_tmp, _keystore_path, unlock_path) = setup_env();
        create_keystore("current-pw").expect("create keystore");
        write_unlock_lease_until("stale-pw", current_epoch_seconds() + 300).expect("write lease");

        let prompted = get_password_for_secret_commands_with_prompt(true, |prompt| {
            assert_eq!(
                prompt,
                "Enter keystore password (ESDIAG_KEYSTORE_PASSWORD): "
            );
            Ok("prompted-pw".to_string())
        })
        .expect("prompted fallback");

        assert_eq!(prompted, "prompted-pw");
        assert!(
            !unlock_path.exists(),
            "stale unlock lease should be cleared before prompting"
        );
    }
}
