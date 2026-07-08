use eyre::{Result, eyre};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tokio::task;

use super::{HostRole, KnownHost, Uri};
use crate::{
    job::model::{Input, Process, SaveTarget, SendTarget},
    processor::{
        Identifiers,
        api::{ApiResolver, ProcessSelection},
    },
};

pub use crate::job::model::ExportTarget as JobOutput;
pub use crate::job::model::Job;
pub use crate::processor::api::ProcessSelection as JobProcessSelection;
pub type SavedJobs = IndexMap<String, Job>;

const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Deserialize, Serialize)]
struct SavedJobsDocument {
    schema_version: u32,
    jobs: SavedJobs,
}

impl SavedJobsDocument {
    fn current(jobs: SavedJobs) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            jobs,
        }
    }
}

#[derive(Clone, Deserialize)]
struct LegacyJob {
    #[serde(default, skip_serializing_if = "Identifiers::is_empty")]
    identifiers: Identifiers,
    collect: LegacyJobCollect,
    #[serde(flatten)]
    action: LegacyJobAction,
}

#[derive(Clone, Deserialize)]
struct LegacyJobCollect {
    host: String,
    #[serde(default = "default_diagnostic_type")]
    diagnostic_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    save_dir: Option<PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum LegacyJobAction {
    Collect {
        output_dir: PathBuf,
    },
    Upload {
        upload_id: String,
    },
    Process {
        output: JobOutput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<LegacyJobProcessSelection>,
    },
}

#[derive(Clone, Deserialize)]
struct LegacyJobProcessSelection {
    #[serde(default = "default_process_product")]
    product: String,
    #[serde(default = "default_diagnostic_type")]
    diagnostic_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    selected: Vec<String>,
}

#[derive(Clone)]
pub struct NeedsCollect;
#[derive(Clone)]
pub struct NeedsAction;

pub struct JobBuilder<State> {
    identifiers: Identifiers,
    collect: Option<LegacyJobCollect>,
    _state: PhantomData<State>,
}

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

impl TryFrom<LegacyJob> for Job {
    type Error = eyre::Report;

    fn try_from(legacy: LegacyJob) -> Result<Self> {
        let LegacyJob {
            identifiers,
            collect,
            action,
        } = legacy;
        let input = Input::Collect {
            host: collect.host,
            diagnostic_type: collect.diagnostic_type,
            include: None,
            exclude: None,
        };
        let retained_save = collect.save_dir.map(SaveTarget::retained);

        let (save, process, send) = match action {
            LegacyJobAction::Collect { output_dir } => (Some(SaveTarget::retained(output_dir)), None, None),
            LegacyJobAction::Upload { upload_id } => (
                Some(retained_save.unwrap_or_else(SaveTarget::temporary)),
                None,
                Some(SendTarget { upload_id }),
            ),
            LegacyJobAction::Process { output, selection } => (
                retained_save,
                Some(Process {
                    selection: canonicalize_legacy_selection(selection)?,
                    export: output,
                }),
                None,
            ),
        };

        Job::try_new(identifiers, input, save, process, send).map_err(Into::into)
    }
}

fn canonicalize_legacy_selection(selection: Option<LegacyJobProcessSelection>) -> Result<Option<ProcessSelection>> {
    let Some(selection) = selection else {
        return Ok(None);
    };
    let selected =
        ApiResolver::resolve_processing_selection(&selection.product, &selection.diagnostic_type, &selection.selected)?;
    Ok(Some(JobProcessSelection {
        product: selection.product,
        diagnostic_type: selection.diagnostic_type,
        selected,
    }))
}

impl Job {
    pub fn builder() -> JobBuilder<NeedsCollect> {
        JobBuilder::new()
    }

    pub fn collect_host(&self) -> &str {
        match self.input() {
            Input::Collect { host, .. } => host,
            Input::Load { .. } => "",
        }
    }

    pub fn processing_label(&self) -> &str {
        self.process()
            .and_then(|process| process.selection.as_ref())
            .map(|selection| selection.diagnostic_type.as_str())
            .unwrap_or_else(|| {
                if self.process().is_some() {
                    "standard"
                } else {
                    "skipped"
                }
            })
    }

    pub fn send_target_label(&self) -> String {
        if let Some(send) = self.send() {
            return send.upload_id.clone();
        }
        if let Some(process) = self.process() {
            return process.export.label();
        }
        self.save()
            .and_then(|save| save.dir.as_ref())
            .map(|dir| format!("dir:{}", dir.display()))
            .unwrap_or_default()
    }

    pub fn referenced_hosts(&self) -> Vec<&str> {
        let mut hosts = Vec::new();
        if let Input::Collect { host, .. } = self.input() {
            hosts.push(host.as_str());
        }
        if let Some(Process {
            export: JobOutput::KnownHost { name },
            ..
        }) = self.process()
        {
            hosts.push(name.as_str());
        }
        hosts
    }

    pub fn to_signals(&self) -> JobSignals {
        let mut signals = JobSignals::default();
        if let Input::Collect {
            host, diagnostic_type, ..
        } = self.input()
        {
            signals.collect.mode = CollectMode::Collect;
            signals.collect.source = CollectSource::KnownHost;
            signals.collect.known_host = host.clone();
            signals.collect.diagnostic_type = diagnostic_type.clone();
        }
        if let Some(save) = self.save() {
            signals.collect.save = save.is_retained();
            signals.collect.download_dir = save
                .dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
        }

        if let Some(process) = self.process() {
            signals.process.enabled = true;
            signals.process.mode = ProcessMode::Process;
            if let Some(selection) = &process.selection {
                signals.process.product = selection.product.clone();
                signals.process.diagnostic_type = selection.diagnostic_type.clone();
                signals.process.advanced = !selection.selected.is_empty();
                signals.process.selected = selection.selected.join(",");
            }
            process.export.apply_to_signals(&mut signals);
        } else if self.send().is_some() {
            signals.process.enabled = false;
            signals.process.mode = ProcessMode::Forward;
        } else {
            signals.process.enabled = false;
            signals.process.mode = ProcessMode::Forward;
            signals.send.mode = SendMode::Local;
            signals.send.local_target = "directory".to_string();
            signals.send.local_directory = signals.collect.download_dir.clone();
        }
        if let Some(send) = self.send() {
            signals.send.mode = SendMode::Remote;
            signals.send.remote_target = send.upload_id.clone();
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

fn intermediate_download_dir(signals: &JobSignals) -> Option<String> {
    (signals.collect.save && !signals.collect.download_dir.trim().is_empty())
        .then(|| signals.collect.download_dir.clone())
}

fn explicit_process_selection(signals: &JobSignals) -> Result<Option<ProcessSelection>> {
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
    if !has_explicit_choice {
        return Ok(None);
    }

    let selected = ApiResolver::resolve_processing_selection(
        &signals.process.product,
        &signals.process.diagnostic_type,
        &selected,
    )?;
    Ok(Some(ProcessSelection {
        product: signals.process.product.clone(),
        diagnostic_type: signals.process.diagnostic_type.clone(),
        selected,
    }))
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
            let selection = explicit_process_selection(&signals)?;
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
            collect: Some(LegacyJobCollect {
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
        self.build(Some(SaveTarget::retained(output_dir)), None, None)
    }

    pub fn upload_to(self, upload_id: impl Into<String>) -> Result<Job> {
        let upload_id = upload_id.into();
        if upload_id.trim().is_empty() {
            return Err(eyre!("Upload jobs require an Elastic Upload Service upload id or URL"));
        }
        let save = self
            .collect
            .as_ref()
            .and_then(|collect| collect.save_dir.clone())
            .map(SaveTarget::retained)
            .unwrap_or_else(SaveTarget::temporary);
        self.build(Some(save), None, Some(SendTarget { upload_id }))
    }

    pub fn process_to(self, output: JobOutput) -> Result<Job> {
        self.process_to_with_selection(output, None)
    }

    pub fn process_to_with_selection(self, output: JobOutput, selection: Option<ProcessSelection>) -> Result<Job> {
        let save = self
            .collect
            .as_ref()
            .and_then(|collect| collect.save_dir.clone())
            .map(SaveTarget::retained);
        self.build(
            save,
            Some(Process {
                selection,
                export: output,
            }),
            None,
        )
    }

    fn collect_mut(&mut self) -> &mut LegacyJobCollect {
        self.collect.as_mut().expect("typestate guarantees collect")
    }

    fn build(self, save: Option<SaveTarget>, process: Option<Process>, send: Option<SendTarget>) -> Result<Job> {
        let collect = self.collect.expect("typestate guarantees collect");
        let input = Input::Collect {
            host: collect.host,
            diagnostic_type: collect.diagnostic_type,
            include: None,
            exclude: None,
        };
        Job::try_new(self.identifiers, input, save, process, send).map_err(Into::into)
    }
}

fn saved_jobs_io_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn load_saved_jobs() -> Result<SavedJobs> {
    let _guard = saved_jobs_io_lock()
        .lock()
        .map_err(|err| eyre!("Saved jobs IO lock poisoned: {err}"))?;
    load_saved_jobs_unlocked()
}

fn load_saved_jobs_unlocked() -> Result<SavedJobs> {
    let path = get_jobs_path()?;
    load_saved_jobs_from_path(&path)
}

fn load_saved_jobs_from_path(path: &Path) -> Result<SavedJobs> {
    if !path.exists() {
        return Ok(SavedJobs::default());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(SavedJobs::default());
    }

    let version = schema_version(&content)?;
    match version {
        None => {
            let legacy_jobs: IndexMap<String, LegacyJob> = serde_yaml::from_str(&content)?;
            let should_rewrite = !legacy_jobs.is_empty();
            let jobs = legacy_jobs
                .into_iter()
                .map(|(name, job)| Job::try_from(job).map(|job| (name, job)))
                .collect::<Result<SavedJobs>>()?;
            if should_rewrite {
                write_saved_jobs_document(path, &jobs)?;
            }
            Ok(jobs)
        }
        Some(CURRENT_SCHEMA_VERSION) => {
            let document: SavedJobsDocument = serde_yaml::from_str(&content)?;
            Ok(document.jobs)
        }
        Some(version) => Err(eyre!("Unsupported saved jobs schema_version {version}")),
    }
}

fn schema_version(content: &str) -> Result<Option<u32>> {
    let value: serde_yaml::Value = serde_yaml::from_str(content)?;
    let Some(mapping) = value.as_mapping() else {
        return Err(eyre!("Saved jobs file must be a YAML mapping"));
    };
    let key = serde_yaml::Value::String("schema_version".to_string());
    let Some(version) = mapping.get(&key) else {
        return Ok(None);
    };

    match version {
        serde_yaml::Value::Number(number) => number
            .as_u64()
            .and_then(|version| u32::try_from(version).ok())
            .map(Some)
            .ok_or_else(|| eyre!("Invalid saved jobs schema_version value")),
        serde_yaml::Value::Mapping(_) => Ok(None),
        _ => Err(eyre!("Invalid saved jobs schema_version value")),
    }
}

pub fn save_saved_jobs(jobs: &SavedJobs) -> Result<()> {
    let _guard = saved_jobs_io_lock()
        .lock()
        .map_err(|err| eyre!("Saved jobs IO lock poisoned: {err}"))?;
    save_saved_jobs_unlocked(jobs)
}

fn save_saved_jobs_unlocked(jobs: &SavedJobs) -> Result<()> {
    let path = get_jobs_path()?;
    write_saved_jobs_document(&path, jobs)
}

fn write_saved_jobs_document(path: &Path, jobs: &SavedJobs) -> Result<()> {
    let document = SavedJobsDocument::current(jobs.clone());
    super::keystore::write_yaml_atomic(path, &document)
}

pub async fn load_saved_jobs_async() -> Result<SavedJobs> {
    task::spawn_blocking(load_saved_jobs)
        .await
        .map_err(|err| eyre::eyre!("Saved jobs load task failed: {err}"))?
}

pub async fn with_saved_jobs_async<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut SavedJobs) -> Result<(T, bool)> + Send + 'static,
{
    task::spawn_blocking(move || {
        let _guard = saved_jobs_io_lock()
            .lock()
            .map_err(|err| eyre!("Saved jobs IO lock poisoned: {err}"))?;
        let mut jobs = load_saved_jobs_unlocked()?;
        let (result, changed) = operation(&mut jobs)?;
        if changed {
            save_saved_jobs_unlocked(&jobs)?;
        }
        Ok(result)
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

    fn save_collect_host(name: &str) {
        crate::data::KnownHostBuilder::new(url::Url::parse("http://localhost:9200/").expect("url"))
            .product(crate::data::Product::Elasticsearch)
            .roles(vec![HostRole::Collect, HostRole::Send])
            .build()
            .expect("host")
            .save(name)
            .expect("save host");
    }

    fn test_job(host: &str) -> Job {
        Job::try_new(
            Identifiers::default(),
            Input::Collect {
                host: host.to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            Some(SaveTarget::retained(PathBuf::from("/tmp/esdiag"))),
            None,
            None,
        )
        .expect("valid job")
    }

    fn write_jobs(path: &Path, yaml: &str) {
        std::fs::write(path, yaml.trim_start()).expect("write jobs");
    }

    #[test]
    fn save_saved_jobs_writes_schema_version() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();

        let mut jobs = SavedJobs::default();
        jobs.insert("first".to_string(), test_job("first"));
        save_saved_jobs(&jobs).expect("save jobs");

        let content = std::fs::read_to_string(tmp.path().join("jobs.yml")).expect("read jobs");
        assert!(content.contains("schema_version: 2"));
        assert!(content.contains("jobs:"));
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
    fn job_serializes_phase_shape() {
        let yaml = serde_yaml::to_string(&test_job("prod")).expect("serialize job");

        assert!(yaml.contains("input:"));
        assert!(yaml.contains("type: collect"));
        assert!(yaml.contains("host: prod"));
        assert!(yaml.contains("save:"));
        assert!(!yaml.contains("action:"));
        assert!(!yaml.contains("collect:"));
        assert!(!yaml.contains("local_target"));
    }

    #[test]
    fn absent_schema_version_is_treated_as_v1_and_migrates_each_action() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
collect-job:
  collect:
    host: prod
    diagnostic_type: support
  action: collect
  output_dir: /tmp/collect
upload-job:
  collect:
    host: prod
    save_dir: /tmp/upload-bundle
  action: upload
  upload_id: upload-123
process-job:
  collect:
    host: prod
    save_dir: /tmp/process-bundle
  action: process
  output:
    type: directory
    output_dir: /tmp/process-output
  selection:
    product: elasticsearch
    diagnostic_type: minimal
    selected:
      - nodes_stats
"#,
        );

        let jobs = load_saved_jobs().expect("load and migrate jobs");

        let collect = jobs.get("collect-job").expect("collect job");
        assert!(
            matches!(collect.input(), Input::Collect { host, diagnostic_type, .. } if host == "prod" && diagnostic_type == "support")
        );
        assert_eq!(
            collect.save().and_then(|save| save.dir.as_ref()),
            Some(&PathBuf::from("/tmp/collect"))
        );
        assert!(collect.process().is_none());
        assert!(collect.send().is_none());

        let upload = jobs.get("upload-job").expect("upload job");
        assert_eq!(
            upload.save().and_then(|save| save.dir.as_ref()),
            Some(&PathBuf::from("/tmp/upload-bundle"))
        );
        assert_eq!(upload.send().expect("send").upload_id, "upload-123");

        let process = jobs.get("process-job").expect("process job");
        assert_eq!(
            process.save().and_then(|save| save.dir.as_ref()),
            Some(&PathBuf::from("/tmp/process-bundle"))
        );
        let process_stage = process.process().expect("process stage");
        assert_eq!(
            process_stage.export,
            JobOutput::Directory {
                output_dir: PathBuf::from("/tmp/process-output")
            }
        );
        assert_eq!(
            process_stage.selection.as_ref().expect("selection").diagnostic_type,
            "minimal"
        );
    }

    #[test]
    fn legacy_process_without_save_dir_migrates_to_streaming() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
streaming-job:
  collect:
    host: prod
  action: process
  output:
    type: stdout
"#,
        );

        let jobs = load_saved_jobs().expect("load jobs");
        let job = jobs.get("streaming-job").expect("job");

        assert!(job.save().is_none());
        assert!(job.process().is_some());
        assert_eq!(job.execution_mode(), crate::job::model::ExecutionMode::Streaming);
    }

    #[test]
    fn legacy_job_named_schema_version_is_not_treated_as_version_marker() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
schema_version:
  collect:
    host: prod
  action: collect
  output_dir: /tmp/collect
"#,
        );

        let jobs = load_saved_jobs().expect("legacy schema_version job migrates");

        assert!(jobs.contains_key("schema_version"));
        let migrated = std::fs::read_to_string(&path).expect("read migrated file");
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("jobs:"));
    }

    #[test]
    fn legacy_process_selection_is_canonicalized_during_migration() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
process-job:
  collect:
    host: prod
  action: process
  output:
    type: stdout
  selection:
    product: logstash
    diagnostic_type: standard
    selected:
      - node
"#,
        );

        let jobs = load_saved_jobs().expect("load and migrate jobs");
        let selection = jobs
            .get("process-job")
            .and_then(|job| job.process())
            .and_then(|process| process.selection.as_ref())
            .expect("selection");

        assert!(selection.selected.iter().any(|selected| selected == "logstash_node"));
    }

    #[test]
    fn legacy_process_selection_defaults_product_and_diagnostic_type() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
process-job:
  collect:
    host: prod
  action: process
  output:
    type: stdout
  selection:
    selected:
      - nodes_stats
"#,
        );

        let jobs = load_saved_jobs().expect("load and migrate jobs");
        let selection = jobs
            .get("process-job")
            .and_then(|job| job.process())
            .and_then(|process| process.selection.as_ref())
            .expect("selection");

        assert_eq!(selection.product, "elasticsearch");
        assert_eq!(selection.diagnostic_type, "standard");
        assert!(selection.selected.iter().any(|selected| selected == "nodes_stats"));
    }

    #[test]
    fn v1_load_rewrites_once_and_second_load_is_direct() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        write_jobs(
            &path,
            r#"
collect-job:
  collect:
    host: prod
  action: collect
  output_dir: /tmp/collect
"#,
        );

        let jobs = load_saved_jobs().expect("first load migrates");
        let migrated = std::fs::read_to_string(&path).expect("read migrated file");
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("jobs:"));
        assert!(!migrated.contains("action: collect"));

        let loaded_again = load_saved_jobs().expect("second load is direct");
        let after_second_load = std::fs::read_to_string(&path).expect("read again");

        assert_eq!(loaded_again.keys().collect::<Vec<_>>(), jobs.keys().collect::<Vec<_>>());
        assert_eq!(after_second_load, migrated);
    }

    #[test]
    fn current_version_loads_directly_without_rewrite() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        let path = tmp.path().join("jobs.yml");
        let mut jobs = SavedJobs::default();
        jobs.insert("current".to_string(), test_job("prod"));
        write_saved_jobs_document(&path, &jobs).expect("write current jobs");
        let before = std::fs::read_to_string(&path).expect("read current jobs");

        let loaded = load_saved_jobs_from_path(&path).expect("load current jobs");
        let after = std::fs::read_to_string(&path).expect("read after load");

        assert!(loaded.contains_key("current"));
        assert_eq!(after, before);
    }

    #[test]
    fn atomic_write_failure_leaves_original_file_intact() {
        struct FailingSerialize;

        impl Serialize for FailingSerialize {
            fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("intentional failure"))
            }
        }

        let tmp = TempDir::new().expect("temp dir");
        let path = tmp.path().join("jobs.yml");
        std::fs::write(&path, "original: true\n").expect("seed original");

        let err = super::super::keystore::write_yaml_atomic(&path, &FailingSerialize)
            .expect_err("write should fail before replace");

        assert!(err.to_string().contains("intentional failure"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read original"),
            "original: true\n"
        );
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
        save_collect_host("prod");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.collect.save = true;
        signals.collect.download_dir = "/tmp/browser-download".to_string();
        signals.process.enabled = false;
        signals.process.mode = ProcessMode::Forward;
        signals.send.mode = SendMode::Local;

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        assert_eq!(
            job.save().and_then(|save| save.dir.as_ref()),
            Some(&PathBuf::from("/tmp/browser-download"))
        );
        assert!(job.process().is_none());
    }

    #[test]
    fn process_directory_output_serializes_as_output_dir() {
        let job = Job::try_new(
            Identifiers::default(),
            Input::Collect {
                host: "prod".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            Some(SaveTarget::retained(PathBuf::from("/tmp/retain-bundle"))),
            Some(Process {
                selection: None,
                export: JobOutput::Directory {
                    output_dir: PathBuf::from("/tmp/final-output"),
                },
            }),
            None,
        )
        .expect("valid job");

        let yaml = serde_yaml::to_string(&job).expect("serialize job");

        assert!(yaml.contains("dir: /tmp/retain-bundle"));
        assert!(yaml.contains("type: directory"));
        assert!(yaml.contains("output_dir: /tmp/final-output"));
        assert!(!yaml.contains("path: /tmp/final-output"));
    }

    #[test]
    fn job_signals_preserve_local_known_host_output() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        save_collect_host("prod");
        save_collect_host("monitoring");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.process.enabled = true;
        signals.process.mode = ProcessMode::Process;
        signals.send.mode = SendMode::Local;
        signals.send.local_target = "monitoring".to_string();

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        match &job.process().expect("process").export {
            JobOutput::KnownHost { name } => assert_eq!(name, "monitoring"),
            _ => panic!("expected process to known host"),
        }
    }

    #[test]
    fn job_signals_preserve_local_directory_output() {
        let _guard = test_env_lock().lock().expect("env lock");
        let tmp = setup_env();
        save_collect_host("prod");
        let output_dir = tmp.path().join("output");
        std::fs::create_dir(&output_dir).expect("create output dir");

        let mut signals = JobSignals::default();
        signals.collect.known_host = "prod".to_string();
        signals.process.enabled = true;
        signals.process.mode = ProcessMode::Process;
        signals.send.mode = SendMode::Local;
        signals.send.local_target = output_dir.display().to_string();

        let job = Job::from_signals(signals, Identifiers::default()).expect("job from signals");

        match &job.process().expect("process").export {
            JobOutput::Directory { output_dir: actual } => assert_eq!(actual, &output_dir),
            _ => panic!("expected process to directory"),
        }
    }

    #[test]
    fn collect_job_requires_output_dir() {
        let _guard = test_env_lock().lock().expect("env lock");
        let _tmp = setup_env();
        save_collect_host("prod");

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
        save_collect_host("prod");

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
