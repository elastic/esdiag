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
    fs::File,
    future::Future,
    io::{BufReader, BufWriter, IsTerminal},
    path::PathBuf,
};

const KDF_ROUNDS: u32 = 100_000;
const KEY_SIZE: usize = 32;
const SALT_SIZE: usize = 16;
const NONCE_SIZE: usize = 12;
const KEYSTORE_FILE: &str = "secrets.yml";

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

pub fn get_keystore_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("ESDIAG_KEYSTORE") {
        return Ok(PathBuf::from(path));
    }
    let home_dir = match std::env::consts::OS {
        "windows" => env::var("USERPROFILE")?,
        "linux" | "macos" => env::var("HOME")?,
        os => return Err(eyre!("Unknown home directory for operating system: {os}")),
    };
    let esdiag_dir = PathBuf::from(home_dir).join(".esdiag");
    if !esdiag_dir.exists() {
        std::fs::create_dir_all(&esdiag_dir)?;
    }
    Ok(esdiag_dir.join(KEYSTORE_FILE))
}

pub fn get_password_from_env() -> Result<String> {
    if let Ok(password) = SCOPED_KEYSTORE_PASSWORD.try_with(Clone::clone) {
        return Ok(password);
    }
    env::var(ESDIAG_KEYSTORE_PASSWORD).map_err(|_| {
        eyre!("{ESDIAG_KEYSTORE_PASSWORD} is not set; cannot decrypt secrets from keystore.")
    })
}

pub async fn with_scoped_keystore_password<F>(keystore_password: String, future: F) -> F::Output
where
    F: Future,
{
    SCOPED_KEYSTORE_PASSWORD
        .scope(keystore_password, future)
        .await
}

pub fn get_password_for_secret_commands() -> Result<String> {
    if let Ok(password) = env::var(ESDIAG_KEYSTORE_PASSWORD) {
        return Ok(password);
    }

    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        let prompt = format!("Enter keystore password ({ESDIAG_KEYSTORE_PASSWORD}): ");
        return Ok(rpassword::prompt_password(prompt)?);
    }

    Err(eyre!(
        "{ESDIAG_KEYSTORE_PASSWORD} is not set and no interactive terminal is available."
    ))
}

pub fn add_secret(
    secret_id: &str,
    username: Option<String>,
    password: Option<String>,
    apikey: Option<String>,
    keystore_password: &str,
) -> Result<String> {
    let auth = match (apikey, username, password) {
        (Some(apikey), None, None) => SecretAuth::ApiKey { apikey },
        (None, Some(username), Some(password)) => SecretAuth::Basic { username, password },
        _ => {
            return Err(eyre!(
                "Invalid secret auth: use either --apikey or --user with --password"
            ));
        }
    };

    upsert_secret_auth(secret_id, auth, keystore_password)?;
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
    let path = get_keystore_path()?;
    if !path.is_file() {
        let store = KeystoreData::default();
        write_store(&store, keystore_password)?;
        tracing::info!("Created empty keystore at {}", path.display());
        return Ok(());
    }

    read_store(keystore_password).map(|_| ())
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
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_yaml::to_writer(writer, &envelope)?;
    Ok(())
}

fn derive_key(password: &str, salt: &[u8]) -> [u8; KEY_SIZE] {
    let mut key = [0_u8; KEY_SIZE];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, KDF_ROUNDS, &mut key);
    key
}
