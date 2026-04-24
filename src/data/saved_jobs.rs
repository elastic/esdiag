use eyre::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Mutex, task};

use super::workflow::Workflow;
use crate::processor::Identifiers;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SavedJob {
    #[serde(default)]
    pub identifiers: Identifiers,
    #[serde(default)]
    pub workflow: Workflow,
}

pub type SavedJobs = IndexMap<String, SavedJob>;

fn saved_jobs_io_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn load_saved_jobs() -> Result<SavedJobs> {
    let path = get_jobs_path()?;
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(SavedJobs::default());
        }
        let jobs: SavedJobs = serde_yaml::from_str(&content)?;
        Ok(jobs)
    } else {
        Ok(SavedJobs::default())
    }
}

pub fn save_saved_jobs(jobs: &SavedJobs) -> Result<()> {
    let path = get_jobs_path()?;
    write_yaml_atomic(&path, jobs)?;
    Ok(())
}

pub async fn load_saved_jobs_async() -> Result<SavedJobs> {
    let _guard = saved_jobs_io_lock().lock().await;
    task::spawn_blocking(load_saved_jobs)
        .await
        .map_err(|err| eyre::eyre!("Saved jobs load task failed: {err}"))?
}

pub async fn with_saved_jobs_async<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut SavedJobs) -> Result<T> + Send + 'static,
{
    let _guard = saved_jobs_io_lock().lock().await;
    task::spawn_blocking(move || {
        let mut jobs = load_saved_jobs()?;
        operation(&mut jobs)
    })
    .await
    .map_err(|err| eyre::eyre!("Saved jobs update task failed: {err}"))?
}

fn get_jobs_path() -> Result<PathBuf> {
    let hosts_path = super::KnownHost::get_hosts_path();
    let esdiag_dir = hosts_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    if !esdiag_dir.exists() {
        fs::create_dir_all(&esdiag_dir)?;
    }
    Ok(esdiag_dir.join("jobs.yml"))
}

fn secure_output_file(path: &Path) -> Result<File> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        options.mode(0o600);
        let file = options.open(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        Ok(options.open(path)?)
    }
}

fn temp_output_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| eyre::eyre!("Path '{}' has no parent directory", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| eyre::eyre!("Path '{}' has no file name", path.display()))?
        .to_string_lossy();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    Ok(parent.join(format!(".{file_name}.tmp-{}-{unique}", std::process::id())))
}

fn replace_file_atomic(path: &Path, temp_path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let backup_path = path.with_extension("bak");
        let mut backup_created = false;

        if path.exists() {
            if backup_path.exists() {
                let _ = fs::remove_file(&backup_path);
            }
            fs::rename(path, &backup_path)?;
            backup_created = true;
        }

        match fs::rename(temp_path, path) {
            Ok(()) => {
                if backup_created {
                    let _ = fs::remove_file(&backup_path);
                }
                return Ok(());
            }
            Err(err) => {
                if backup_created && !path.exists() {
                    let _ = fs::rename(&backup_path, path);
                }
                return Err(err.into());
            }
        }
    }

    #[cfg(not(windows))]
    fs::rename(temp_path, path)?;
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
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock;
    use tempfile::TempDir;

    fn setup_env() -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        let hosts = tmp.path().join("hosts.yml");
        unsafe {
            std::env::set_var("ESDIAG_HOSTS", &hosts);
        }
        tmp
    }

    #[test]
    fn save_saved_jobs_overwrites_existing_file() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut jobs = SavedJobs::default();
        jobs.insert("first".to_string(), SavedJob::default());
        save_saved_jobs(&jobs).expect("save initial jobs");

        let mut updated_jobs = SavedJobs::default();
        updated_jobs.insert("second".to_string(), SavedJob::default());
        save_saved_jobs(&updated_jobs).expect("overwrite jobs");

        let loaded_jobs = load_saved_jobs().expect("load saved jobs");
        assert!(loaded_jobs.contains_key("second"));
        assert!(!loaded_jobs.contains_key("first"));
    }
}
