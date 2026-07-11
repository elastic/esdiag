use eyre::{Result, eyre};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Mutex, task};

use super::{HostRole, KnownHost, Uri};
use crate::processor::Identifiers;

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default, skip_serializing_if = "Identifiers::is_empty")]
    pub identifiers: Identifiers,
    pub collect: JobCollect,
    #[serde(flatten)]
    pub action: JobAction,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JobCollect {
    pub host: String,
    #[serde(default = "default_diagnostic_type")]
    pub diagnostic_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dir: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum JobAction {
    Collect {
        output_dir: PathBuf,
    },
    Upload {
        upload_id: String,
    },
    Process {
        output: JobOutput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<JobProcessSelection>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum JobOutput {
    KnownHost { name: String },
    File { path: PathBuf },
    Directory { output_dir: PathBuf },
    Stdout,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JobProcessSelection {
    #[serde(default = "default_process_product")]
    pub product: String,
    #[serde(default = "default_diagnostic_type")]
    pub diagnostic_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<String>,
}

#[derive(Clone)]
pub struct NeedsCollect;
#[derive(Clone)]
pub struct NeedsAction;

pub struct JobBuilder<State> {
    identifiers: Identifiers,
    collect: Option<JobCollect>,
    _state: PhantomData<State>,
}

pub type SavedJobs = IndexMap<String, Job>;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct JobSignals {
    pub collect: JobSignalsCollect,
    pub process: JobSignalsProcess,
    pub send: JobSignalsSend,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct JobSignalsCollect {
    pub mode: CollectMode,
    pub source: CollectSource,
    #[serde(default)]
    pub known_host: String,
    #[serde(default = "default_diagnostic_type")]
    pub diagnostic_type: String,
    #[serde(default)]
    pub save: bool,
    #[serde(default)]
    pub download_dir: String,
}

impl Default for JobSignalsCollect {
    fn default() -> Self {
        Self {
            mode: CollectMode::Collect,
            source: CollectSource::KnownHost,
            known_host: String::new(),
            diagnostic_type: default_diagnostic_type(),
            save: false,
            download_dir: String::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct JobSignalsProcess {
    pub mode: ProcessMode,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_process_product")]
    pub product: String,
    #[serde(default = "default_diagnostic_type")]
    pub diagnostic_type: String,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub selected: String,
}

impl Default for JobSignalsProcess {
    fn default() -> Self {
        Self {
            mode: ProcessMode::Process,
            enabled: true,
            product: default_process_product(),
            diagnostic_type: default_diagnostic_type(),
            advanced: false,
            selected: String::new(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct JobSignalsSend {
    pub mode: SendMode,
    #[serde(default)]
    pub remote_target: String,
    #[serde(default)]
    pub local_target: String,
    #[serde(default)]
    pub local_directory: String,
}

impl Default for JobSignalsSend {
    fn default() -> Self {
        Self {
            mode: SendMode::Remote,
            remote_target: String::new(),
            local_target: String::new(),
            local_directory: String::new(),
        }
    }
}

fn default_process_product() -> String {
    "elasticsearch".to_string()
}

fn default_diagnostic_type() -> String {
    "standard".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CollectMode {
    Collect,
    Upload,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CollectSource {
    KnownHost,
    ApiKey,
    ServiceLink,
    UploadFile,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessMode {
    Process,
    Forward,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SendMode {
    Remote,
    Local,
}

impl Job {
    pub fn builder() -> JobBuilder<NeedsCollect> {
        JobBuilder::new()
    }

    pub fn collect_host(&self) -> &str {
        &self.collect.host
    }

    pub fn processing_label(&self) -> &str {
        match &self.action {
            JobAction::Process { selection, .. } => selection
                .as_ref()
                .map(|selection| selection.diagnostic_type.as_str())
                .unwrap_or("standard"),
            JobAction::Collect { .. } | JobAction::Upload { .. } => "skipped",
        }
    }

    pub fn send_target_label(&self) -> String {
        match &self.action {
            JobAction::Collect { output_dir } => format!("dir:{}", output_dir.display()),
            JobAction::Upload { upload_id } => upload_id.clone(),
            JobAction::Process { output, .. } => output.label(),
        }
    }

    pub fn referenced_hosts(&self) -> Vec<&str> {
        let mut hosts = vec![self.collect.host.as_str()];
        if let JobAction::Process {
            output: JobOutput::KnownHost { name },
            ..
        } = &self.action
        {
            hosts.push(name.as_str());
        }
        hosts
    }

    pub fn to_signals(&self) -> JobSignals {
        let mut signals = JobSignals::default();
        signals.collect.mode = CollectMode::Collect;
        signals.collect.source = CollectSource::KnownHost;
        signals.collect.known_host = self.collect.host.clone();
        signals.collect.diagnostic_type = self.collect.diagnostic_type.clone();
        signals.collect.save = self.collect.save_dir.is_some();
        signals.collect.download_dir = self
            .collect
            .save_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        match &self.action {
            JobAction::Collect { output_dir } => {
                signals.collect.save = true;
                signals.collect.download_dir = output_dir.display().to_string();
                signals.process.enabled = false;
                signals.process.mode = ProcessMode::Forward;
                signals.send.mode = SendMode::Local;
                signals.send.local_target = "directory".to_string();
                signals.send.local_directory = output_dir.display().to_string();
            }
            JobAction::Upload { upload_id } => {
                signals.process.enabled = false;
                signals.process.mode = ProcessMode::Forward;
                signals.send.mode = SendMode::Remote;
                signals.send.remote_target = upload_id.clone();
            }
            JobAction::Process { output, selection } => {
                signals.process.enabled = true;
                signals.process.mode = ProcessMode::Process;
                if let Some(selection) = selection {
                    signals.process.product = selection.product.clone();
                    signals.process.diagnostic_type = selection.diagnostic_type.clone();
                    signals.process.advanced = !selection.selected.is_empty();
                    signals.process.selected = selection.selected.join(",");
                }
                output.apply_to_signals(&mut signals);
            }
        }

        signals
    }

    pub fn from_signals(signals: JobSignals, identifiers: Identifiers) -> Result<Self> {
        Job::builder().identifiers(identifiers).from_signals(signals)
    }
}

impl TryFrom<JobSignals> for Job {
    type Error = eyre::Report;

    fn try_from(signals: JobSignals) -> Result<Self> {
        Job::from_signals(signals, Identifiers::default())
    }
}

impl From<&Job> for JobSignals {
    fn from(job: &Job) -> Self {
        job.to_signals()
    }
}

impl JobOutput {
    pub fn from_cli_target(target: &str) -> Result<Self> {
        match Uri::try_from(target.to_string())? {
            Uri::KnownHost(_) | Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_) => Ok(Self::KnownHost {
                name: target.to_string(),
            }),
            Uri::Directory(output_dir) => Ok(Self::Directory { output_dir }),
            Uri::File(path) => Ok(Self::File { path }),
            Uri::Stream => Ok(Self::Stdout),
            _ => Err(eyre!(
                "Jobs require an explicit known host or local filesystem output target"
            )),
        }
    }

    pub fn target_uri(&self) -> String {
        match self {
            Self::KnownHost { name } => name.clone(),
            Self::File { path } => path.display().to_string(),
            Self::Directory { output_dir } => output_dir.display().to_string(),
            Self::Stdout => "-".to_string(),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::KnownHost { name } => name.clone(),
            Self::File { path } => path.display().to_string(),
            Self::Directory { output_dir } => format!("dir:{}", output_dir.display()),
            Self::Stdout => "stdout".to_string(),
        }
    }

    fn from_signals_send(signals: &JobSignals) -> Result<Self> {
        match signals.send.mode {
            SendMode::Remote => {
                let target = signals.send.remote_target.trim();
                if target.is_empty() {
                    return Err(eyre!("Process jobs require a remote output target"));
                }
                Ok(Self::KnownHost {
                    name: target.to_string(),
                })
            }
            SendMode::Local => {
                if signals.send.local_target == "directory" {
                    let directory = signals.send.local_directory.trim();
                    if directory.is_empty() {
                        return Err(eyre!("Process jobs require a local output directory"));
                    }
                    Ok(Self::Directory {
                        output_dir: PathBuf::from(directory),
                    })
                } else {
                    let target = signals.send.local_target.trim();
                    if target.is_empty() {
                        return Err(eyre!("Process jobs require a local output target"));
                    }
                    match Uri::try_from(target.to_string())? {
                        Uri::KnownHost(_) | Uri::ElasticCloudAdmin(_) | Uri::ElasticGovCloudAdmin(_) => {
                            Ok(Self::KnownHost {
                                name: target.to_string(),
                            })
                        }
                        Uri::Stream => Ok(Self::Stdout),
                        Uri::File(path) => Ok(Self::File { path }),
                        Uri::Directory(output_dir) => Ok(Self::Directory { output_dir }),
                        _ => Err(eyre!(
                            "Jobs require an explicit known host or local filesystem output target"
                        )),
                    }
                }
            }
        }
    }

    fn apply_to_signals(&self, signals: &mut JobSignals) {
        match self {
            Self::KnownHost { name } => {
                signals.send.mode = SendMode::Remote;
                signals.send.remote_target = name.clone();
            }
            Self::Directory { output_dir } => {
                signals.send.mode = SendMode::Local;
                signals.send.local_target = "directory".to_string();
                signals.send.local_directory = output_dir.display().to_string();
            }
            Self::File { path } => {
                signals.send.mode = SendMode::Local;
                signals.send.local_target = path.display().to_string();
            }
            Self::Stdout => {
                signals.send.mode = SendMode::Local;
                signals.send.local_target = "-".to_string();
            }
        }
    }
}

impl JobProcessSelection {
    fn from_signals(signals: &JobSignals) -> Option<Self> {
        let selected: Vec<String> = signals
            .process
            .selected
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();
        let has_explicit_choice = !selected.is_empty()
            || signals.process.product != "elasticsearch"
            || signals.process.diagnostic_type != "standard";
        has_explicit_choice.then(|| Self {
            product: signals.process.product.clone(),
            diagnostic_type: signals.process.diagnostic_type.clone(),
            selected,
        })
    }
}

fn intermediate_download_dir(signals: &JobSignals) -> Option<String> {
    (signals.collect.save && !signals.collect.download_dir.trim().is_empty())
        .then(|| signals.collect.download_dir.clone())
}

impl JobBuilder<NeedsCollect> {
    pub fn new() -> Self {
        Self {
            identifiers: Identifiers::default(),
            collect: None,
            _state: PhantomData,
        }
    }

    pub fn identifiers(mut self, identifiers: Identifiers) -> Self {
        self.identifiers = identifiers;
        self
    }

    pub fn from_signals(self, signals: JobSignals) -> Result<Job> {
        if signals.collect.mode != CollectMode::Collect {
            return Err(eyre!("Jobs require collect mode"));
        }
        if signals.collect.source != CollectSource::KnownHost {
            return Err(eyre!("Jobs require a known-host collection source"));
        }

        let mut builder = self
            .collect_from(signals.collect.known_host.clone())?
            .diagnostic_type(signals.collect.diagnostic_type.clone());

        if signals.process.enabled && signals.process.mode == ProcessMode::Process {
            if let Some(download_dir) = intermediate_download_dir(&signals) {
                builder = builder.save_collected_bundle_to(download_dir);
            }
            let output = JobOutput::from_signals_send(&signals)?;
            let selection = JobProcessSelection::from_signals(&signals);
            builder.process_to_with_selection(output, selection)
        } else if signals.process.mode == ProcessMode::Forward && signals.send.mode == SendMode::Remote {
            if let Some(download_dir) = intermediate_download_dir(&signals) {
                builder = builder.save_collected_bundle_to(download_dir);
            }
            builder.upload_to(signals.send.remote_target)
        } else {
            let output_dir = if signals.collect.save && !signals.collect.download_dir.trim().is_empty() {
                signals.collect.download_dir
            } else if signals.send.local_target == "directory" && !signals.send.local_directory.trim().is_empty() {
                signals.send.local_directory
            } else {
                return Err(eyre!("Collect-only jobs require an output directory"));
            };
            builder.collect_to(output_dir)
        }
    }

    pub fn collect_from(self, host: impl Into<String>) -> Result<JobBuilder<NeedsAction>> {
        let host = host.into();
        let host_name = host.trim();
        let known_host = KnownHost::get_known(&host_name.to_string())
            .ok_or_else(|| eyre!("Jobs require a saved known host name as input"))?;
        if !known_host.has_role(HostRole::Collect) {
            return Err(eyre!(
                "Host role validation failed for job input: required role 'collect' not present"
            ));
        }
        Ok(JobBuilder {
            identifiers: self.identifiers,
            collect: Some(JobCollect {
                host: host_name.to_string(),
                diagnostic_type: default_diagnostic_type(),
                save_dir: None,
            }),
            _state: PhantomData,
        })
    }
}

impl Default for JobBuilder<NeedsCollect> {
    fn default() -> Self {
        Self::new()
    }
}

impl JobBuilder<NeedsAction> {
    pub fn identifiers(mut self, identifiers: Identifiers) -> Self {
        self.identifiers = identifiers;
        self
    }

    pub fn diagnostic_type(mut self, diagnostic_type: impl Into<String>) -> Self {
        self.collect_mut().diagnostic_type = diagnostic_type.into();
        self
    }

    pub fn save_collected_bundle_to(mut self, save_dir: impl Into<PathBuf>) -> Self {
        self.collect_mut().save_dir = Some(save_dir.into());
        self
    }

    pub fn collect_to(self, output_dir: impl Into<PathBuf>) -> Result<Job> {
        let output_dir = output_dir.into();
        if output_dir.as_os_str().is_empty() {
            return Err(eyre!("Collect jobs require an output directory"));
        }
        if self
            .collect
            .as_ref()
            .and_then(|collect| collect.save_dir.as_ref())
            .is_some()
        {
            return Err(eyre!(
                "Collect jobs use output_dir as their final diagnostic bundle destination"
            ));
        }
        self.build_action(JobAction::Collect { output_dir })
    }

    pub fn upload_to(self, upload_id: impl Into<String>) -> Result<Job> {
        let upload_id = upload_id.into();
        if upload_id.trim().is_empty() {
            return Err(eyre!("Upload jobs require an Elastic Upload Service upload id or URL"));
        }
        self.build_action(JobAction::Upload { upload_id })
    }

    pub fn process_to(self, output: JobOutput) -> Result<Job> {
        self.process_to_with_selection(output, None)
    }

    pub fn process_to_with_selection(self, output: JobOutput, selection: Option<JobProcessSelection>) -> Result<Job> {
        self.build_action(JobAction::Process { output, selection })
    }

    fn collect_mut(&mut self) -> &mut JobCollect {
        self.collect.as_mut().expect("typestate guarantees collect")
    }

    fn build_action(self, action: JobAction) -> Result<Job> {
        Ok(Job {
            identifiers: self.identifiers,
            collect: self.collect.expect("typestate guarantees collect"),
            action,
        })
    }
}

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

    fn test_job(host: &str) -> Job {
        Job {
            identifiers: Identifiers::default(),
            collect: JobCollect {
                host: host.to_string(),
                diagnostic_type: "standard".to_string(),
                save_dir: None,
            },
            action: JobAction::Collect {
                output_dir: PathBuf::from("/tmp/esdiag"),
            },
        }
    }

    #[test]
    fn save_saved_jobs_overwrites_existing_file() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let mut jobs = SavedJobs::default();
        jobs.insert("first".to_string(), test_job("first"));
        save_saved_jobs(&jobs).expect("save initial jobs");

        let mut updated_jobs = SavedJobs::default();
        updated_jobs.insert("second".to_string(), test_job("second"));
        save_saved_jobs(&updated_jobs).expect("overwrite jobs");

        let loaded_jobs = load_saved_jobs().expect("load saved jobs");
        assert!(loaded_jobs.contains_key("second"));
        assert!(!loaded_jobs.contains_key("first"));
    }

    #[test]
    fn job_serializes_typed_action_shape() {
        let yaml = serde_yaml::to_string(&test_job("prod")).expect("serialize job");

        assert!(yaml.contains("host: prod"));
        assert!(yaml.contains("action: collect"));
        assert!(yaml.contains("output_dir: /tmp/esdiag"));
        assert!(!yaml.contains("save_dir"));
        assert!(!yaml.contains("job:"));
        assert!(!yaml.contains("local_target"));
    }

    #[test]
    fn job_signals_deserialize_with_missing_process_and_send_fields() {
        let job: JobSignals = serde_json::from_value(serde_json::json!({
            "collect": {
                "mode": "upload",
                "source": "upload-file",
                "save": false
            }
        }))
        .expect("job signals should deserialize with defaults");

        assert!(job.collect.mode == CollectMode::Upload);
        assert!(job.collect.source == CollectSource::UploadFile);
        assert!(job.process.enabled);
        assert!(job.send.mode == SendMode::Remote);
    }

    #[test]
    fn job_projects_to_signals_for_form_state() {
        let signals = test_job("prod").to_signals();

        assert_eq!(signals.collect.known_host, "prod");
        assert_eq!(signals.collect.diagnostic_type, "standard");
        assert!(signals.collect.save);
        assert_eq!(signals.collect.download_dir, "/tmp/esdiag");
        assert!(!signals.process.enabled);
        assert_eq!(signals.send.local_target, "directory");
        assert_eq!(signals.send.local_directory, "/tmp/esdiag");
    }

    #[test]
    fn collect_only_job_uses_download_dir_as_output_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .expect("host")
            .save("prod")
            .expect("save collect host");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.collect.save = true;
        signals.collect.download_dir = "/tmp/browser-download".to_string();
        signals.process.enabled = false;
        signals.process.mode = ProcessMode::Forward;
        signals.send.mode = SendMode::Local;

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        match job.action {
            JobAction::Collect { output_dir } => assert_eq!(output_dir, PathBuf::from("/tmp/browser-download")),
            _ => panic!("expected collect action"),
        }
        assert!(job.collect.save_dir.is_none());
    }

    #[test]
    fn process_directory_output_serializes_as_output_dir() {
        let job = Job {
            identifiers: Identifiers::default(),
            collect: JobCollect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                save_dir: Some(PathBuf::from("/tmp/retain-bundle")),
            },
            action: JobAction::Process {
                output: JobOutput::Directory {
                    output_dir: PathBuf::from("/tmp/final-output"),
                },
                selection: None,
            },
        };

        let yaml = serde_yaml::to_string(&job).expect("serialize job");

        assert!(yaml.contains("save_dir: /tmp/retain-bundle"));
        assert!(yaml.contains("type: directory"));
        assert!(yaml.contains("output_dir: /tmp/final-output"));
        assert!(!yaml.contains("path: /tmp/final-output"));
    }

    #[test]
    fn job_signals_preserve_local_known_host_output() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .expect("host")
            .save("prod")
            .expect("save collect host");
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9201/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .roles(vec![HostRole::Send])
            .build()
            .expect("host")
            .save("monitoring")
            .expect("save send host");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.process.enabled = true;
        signals.process.mode = ProcessMode::Process;
        signals.send.mode = SendMode::Local;
        signals.send.local_target = "monitoring".to_string();

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        match job.action {
            JobAction::Process {
                output: JobOutput::KnownHost { name },
                ..
            } => assert_eq!(name, "monitoring"),
            _ => panic!("expected process to known host"),
        }
    }

    #[test]
    fn job_signals_preserve_local_directory_output() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let output_dir = tmp.path().join("output");
        std::fs::create_dir(&output_dir).expect("create output dir");
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .roles(vec![HostRole::Collect])
            .build()
            .expect("host")
            .save("prod")
            .expect("save collect host");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.process.enabled = true;
        signals.process.mode = ProcessMode::Process;
        signals.send.mode = SendMode::Local;
        signals.send.local_target = output_dir.display().to_string();

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        match job.action {
            JobAction::Process {
                output: JobOutput::Directory { output_dir: actual },
                ..
            } => assert_eq!(actual, output_dir),
            _ => panic!("expected process to directory"),
        }
    }

    #[test]
    fn collect_job_requires_output_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .build()
            .expect("host")
            .save("prod")
            .expect("save host");

        let err = match Job::builder().collect_from("prod").expect("known host").collect_to("") {
            Ok(_) => panic!("empty output directories should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("Collect jobs require an output directory"));
    }

    #[test]
    fn collect_job_rejects_separate_save_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .build()
            .expect("host")
            .save("prod")
            .expect("save host");

        let err = match Job::builder()
            .collect_from("prod")
            .expect("known host")
            .save_collected_bundle_to("/tmp/retain")
            .collect_to("/tmp/output")
        {
            Ok(_) => panic!("collect jobs should not carry a separate save_dir"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("Collect jobs use output_dir as their final diagnostic bundle destination")
        );
    }

    #[test]
    fn job_builder_rejects_unknown_collect_host() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();

        let err = match Job::builder().collect_from("missing") {
            Ok(_) => panic!("unknown hosts should be rejected"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("Jobs require a saved known host name as input")
        );
    }
}
