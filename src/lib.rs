// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// Shared client libraries for remote connections
pub mod client;
/// Data structures and types for serializing and deserializing
pub mod data;
/// Embedded assets
pub mod embeds;
/// Environment variables
pub mod env;
/// Exports data to various destinations
pub mod exporter;
/// Shared job runner for saved diagnostic jobs
#[cfg(feature = "keystore")]
pub mod job;
/// Data transformation and processing logic
pub mod processor;
/// Receive data from various sources
pub mod receiver;
/// Serve the ESDiag http API
#[cfg(feature = "server")]
pub mod server;
/// Send pre-built assets (index templates, etc) to Elasticsearch
#[cfg(feature = "setup")]
pub mod setup;
/// Upload raw diagnostic archives to Elastic Upload Service
pub mod uploader;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) struct TestEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    pub tmp: tempfile::TempDir,
    pub hosts_path: std::path::PathBuf,
    pub keystore_path: std::path::PathBuf,
    pub settings_path: std::path::PathBuf,
}

#[cfg(test)]
impl TestEnv {
    pub(crate) fn new() -> Self {
        let guard = test_env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let config_dir = tmp.path().join(".esdiag");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let hosts_path = config_dir.join("hosts.yml");
        let keystore_path = config_dir.join("secrets.yml");
        let settings_path = config_dir.join("settings.yml");

        let mut env = Self {
            _guard: guard,
            previous: Vec::new(),
            tmp,
            hosts_path,
            keystore_path,
            settings_path,
        };
        env.set_path("HOME", env.tmp.path().to_path_buf());
        env.set_path("USERPROFILE", env.tmp.path().to_path_buf());
        env.set_path("ESDIAG_HOSTS", env.hosts_path.clone());
        env.set_path("ESDIAG_KEYSTORE", env.keystore_path.clone());
        env.set_path("ESDIAG_SETTINGS", env.settings_path.clone());
        env.remove("ESDIAG_KEYSTORE_PASSWORD");
        env
    }

    pub(crate) fn set_path(&mut self, key: &'static str, value: std::path::PathBuf) {
        self.capture(key);
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub(crate) fn set(&mut self, key: &'static str, value: &str) {
        self.capture(key);
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub(crate) fn remove(&mut self, key: &'static str) {
        self.capture(key);
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn capture(&mut self, key: &'static str) {
        if self.previous.iter().any(|(existing, _)| *existing == key) {
            return;
        }
        self.previous.push((key, std::env::var_os(key)));
    }
}

#[cfg(test)]
impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..).rev() {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
