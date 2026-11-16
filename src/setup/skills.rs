// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Offline, ownership-safe installation of the embedded portable skill.

use crate::embeds::EsdiagSkillAssets;
use eyre::{Context, Result, eyre};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const INSTALLER_MARKER: &str = ".esdiag-installer.json";
const INSTALLER_ID: &str = "esdiag";

/// Coding agents that have a documented user-scoped skill directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTarget {
    Claude,
    Codex,
    OpenCode,
}

impl std::fmt::Display for SkillTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Codex => write!(f, "codex"),
            Self::OpenCode => write!(f, "opencode"),
        }
    }
}

/// Environment inputs for user-scope skill target resolution.
///
/// Keeping this data separate from process environment access makes path
/// precedence deterministic and fixture-testable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillEnvironment {
    pub home: Option<PathBuf>,
    pub claude_config_dir: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
    pub opencode_config_dir: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl SkillEnvironment {
    pub fn current() -> Self {
        Self {
            home: home_dir(),
            claude_config_dir: env_path("CLAUDE_CONFIG_DIR"),
            codex_home: env_path("CODEX_HOME"),
            opencode_config_dir: env_path("OPENCODE_CONFIG_DIR"),
            xdg_config_home: env_path("XDG_CONFIG_HOME"),
        }
    }

    fn home(&self) -> Result<&Path> {
        self.home
            .as_deref()
            .ok_or_else(|| eyre!("Could not determine the current user's home directory"))
    }
}

impl SkillTarget {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::OpenCode];

    /// Resolves the documented user-scoped parent directory for this target.
    pub fn skill_root(self, environment: &SkillEnvironment) -> Result<PathBuf> {
        let home = environment.home()?;
        Ok(match self {
            Self::Claude => environment
                .claude_config_dir
                .clone()
                .unwrap_or_else(|| home.join(".claude"))
                .join("skills"),
            // Codex discovers global skills from ~/.agents/skills. CODEX_HOME
            // remains a positive installation signal but does not change this
            // documented shared Agent Skills location.
            Self::Codex => home.join(".agents").join("skills"),
            Self::OpenCode => environment
                .opencode_config_dir
                .clone()
                .or_else(|| environment.xdg_config_home.as_ref().map(|path| path.join("opencode")))
                .unwrap_or_else(|| home.join(".config").join("opencode"))
                .join("skills"),
        })
    }

    /// Resolves the `esdiag` skill directory for this target.
    pub fn destination(self, environment: &SkillEnvironment) -> Result<PathBuf> {
        Ok(self.skill_root(environment)?.join("esdiag"))
    }

    /// Detects whether an agent has an existing local user installation.
    ///
    /// Detection is deliberately read-only. Explicit target selection can
    /// install into an absent, but documented, user-scoped location.
    pub fn detected(self, environment: &SkillEnvironment) -> bool {
        match self {
            Self::Claude => self
                .skill_root(environment)
                .ok()
                .and_then(|root| root.parent().map(Path::to_path_buf))
                .is_some_and(|home| home.is_dir()),
            Self::Codex => {
                environment.codex_home.as_ref().is_some_and(|path| path.is_dir())
                    || environment
                        .home
                        .as_ref()
                        .is_some_and(|home| home.join(".codex").is_dir())
            }
            Self::OpenCode => self
                .skill_root(environment)
                .ok()
                .and_then(|root| root.parent().map(Path::to_path_buf))
                .is_some_and(|home| home.is_dir()),
        }
    }
}

/// The script-free skill files compiled into the current executable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedSkill {
    files: BTreeMap<PathBuf, Vec<u8>>,
    digest: String,
}

impl EmbeddedSkill {
    /// Loads the canonical, version-matched skill from `rust-embed`.
    pub fn current() -> Result<Self> {
        let mut files = BTreeMap::new();
        for name in EsdiagSkillAssets::iter() {
            let path = skill_path(name.as_ref())?;
            let asset = EsdiagSkillAssets::get(name.as_ref())
                .ok_or_else(|| eyre!("Embedded skill asset disappeared: {}", name.as_ref()))?;
            files.insert(path, asset.data.into_owned());
        }
        Self::from_files(files)
    }

    /// Deterministic digest across sorted relative paths and exact bytes.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn files(&self) -> &BTreeMap<PathBuf, Vec<u8>> {
        &self.files
    }

    fn from_files(files: BTreeMap<PathBuf, Vec<u8>>) -> Result<Self> {
        if !files.contains_key(Path::new("SKILL.md")) {
            return Err(eyre!("Embedded ESDiag skill is missing SKILL.md"));
        }
        if files.keys().any(|path| path.starts_with("scripts")) {
            return Err(eyre!("Embedded ESDiag skill must not include scripts"));
        }
        let digest = manifest_digest(&files);
        Ok(Self { files, digest })
    }
}

/// Read-only classification of a target directory before mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    Exact,
    ManagedIntact { version: String },
    Modified,
    Unrecognized,
}

/// The result of one target installation attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillAction {
    Installed,
    Updated,
    Unchanged,
    ForceUpdated,
    Conflict,
}

impl SkillAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
            Self::ForceUpdated => "force_updated",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillInstallResult {
    pub action: SkillAction,
    pub destination: PathBuf,
}

/// Selects every currently detected supported agent target without mutation.
pub fn detected_targets(environment: &SkillEnvironment) -> Vec<SkillTarget> {
    SkillTarget::ALL
        .into_iter()
        .filter(|target| target.detected(environment))
        .collect()
}

/// Inspect a target without creating directories or changing files.
pub fn inspect(destination: &Path, embedded: &EmbeddedSkill) -> Result<SkillState> {
    if !destination.exists() {
        return Ok(SkillState::Missing);
    }
    let metadata =
        fs::symlink_metadata(destination).wrap_err_with(|| format!("Failed to inspect {}", destination.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(SkillState::Unrecognized);
    }

    let files = read_skill_files(destination)?;
    let marker = read_marker(destination)?;
    if files == *embedded.files() {
        return Ok(SkillState::Exact);
    }
    match marker {
        Some(marker) if marker.installer == INSTALLER_ID && marker.digest == manifest_digest(&files) => {
            Ok(SkillState::ManagedIntact {
                version: marker.version,
            })
        }
        Some(marker) if marker.installer == INSTALLER_ID => Ok(SkillState::Modified),
        _ => Ok(SkillState::Unrecognized),
    }
}

/// Stages, validates, and replaces one skill directory safely.
///
/// Existing user-owned or locally modified directories are never touched
/// without `force`. Replacements use sibling directories and atomic rename on
/// filesystems that support it; the original is restored if replacement fails.
pub fn install(destination: &Path, embedded: &EmbeddedSkill, force: bool) -> Result<SkillInstallResult> {
    let current = inspect(destination, embedded)?;
    let action = match current {
        SkillState::Missing => SkillAction::Installed,
        SkillState::Exact => {
            return Ok(SkillInstallResult {
                action: SkillAction::Unchanged,
                destination: destination.to_path_buf(),
            });
        }
        SkillState::ManagedIntact { .. } => SkillAction::Updated,
        SkillState::Modified | SkillState::Unrecognized if force => SkillAction::ForceUpdated,
        SkillState::Modified | SkillState::Unrecognized => {
            return Ok(SkillInstallResult {
                action: SkillAction::Conflict,
                destination: destination.to_path_buf(),
            });
        }
    };

    let parent = destination
        .parent()
        .ok_or_else(|| eyre!("Skill destination has no parent: {}", destination.display()))?;
    fs::create_dir_all(parent).wrap_err_with(|| format!("Failed to create {}", parent.display()))?;
    let stage = sibling_path(destination, "stage");
    write_staged_skill(&stage, embedded)?;
    match inspect(&stage, embedded)? {
        SkillState::Exact => {}
        state => return Err(eyre!("Staged ESDiag skill failed validation: {state:?}")),
    }

    let backup = destination.exists().then(|| sibling_path(destination, "backup"));
    if let Some(backup) = &backup {
        fs::rename(destination, backup).wrap_err_with(|| {
            format!(
                "Failed to preserve existing ESDiag skill {} before replacement",
                destination.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&stage, destination) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, destination);
        }
        let _ = fs::remove_dir_all(&stage);
        return Err(error).wrap_err_with(|| format!("Failed to activate staged skill at {}", destination.display()));
    }
    if let Some(backup) = backup {
        fs::remove_dir_all(&backup)
            .wrap_err_with(|| format!("Installed skill but could not remove backup {}", backup.display()))?;
    }

    Ok(SkillInstallResult {
        action,
        destination: destination.to_path_buf(),
    })
}

#[derive(Debug, Deserialize, Serialize)]
struct InstallerMarker {
    installer: String,
    version: String,
    digest: String,
}

fn write_staged_skill(stage: &Path, embedded: &EmbeddedSkill) -> Result<()> {
    if stage.exists() {
        return Err(eyre!(
            "Refusing to reuse existing skill staging directory {}",
            stage.display()
        ));
    }
    fs::create_dir(stage).wrap_err_with(|| format!("Failed to create {}", stage.display()))?;
    for (path, contents) in embedded.files() {
        let destination = stage.join(path);
        let parent = destination.parent().expect("skill file has a parent");
        fs::create_dir_all(parent)?;
        let mut file = fs::File::create(&destination)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    // The marker is written last, after every content file exists and has been
    // flushed, so its presence is a reliable installer ownership signal.
    let marker = InstallerMarker {
        installer: INSTALLER_ID.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: embedded.digest().to_owned(),
    };
    let marker_path = stage.join(INSTALLER_MARKER);
    let marker_bytes = serde_json::to_vec_pretty(&marker)?;
    let mut marker_file = fs::File::create(marker_path)?;
    marker_file.write_all(&marker_bytes)?;
    marker_file.write_all(b"\n")?;
    marker_file.sync_all()?;
    Ok(())
}

fn read_marker(destination: &Path) -> Result<Option<InstallerMarker>> {
    let path = destination.join(INSTALLER_MARKER);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(None);
    }
    Ok(serde_json::from_slice(&fs::read(path)?).ok())
}

fn read_skill_files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();
    collect_skill_files(root, root, &mut files)?;
    Ok(files)
}

fn collect_skill_files(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(eyre!("Refusing symlink in installed skill: {}", path.display()));
        }
        if metadata.is_dir() {
            collect_skill_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("child path is beneath skill root")
                .to_path_buf();
            if relative != Path::new(INSTALLER_MARKER) {
                files.insert(relative, fs::read(path)?);
            }
        } else {
            return Err(eyre!("Unsupported installed skill entry: {}", path.display()));
        }
    }
    Ok(())
}

fn manifest_digest(files: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    let mut digest = Sha256::new();
    for (path, contents) in files {
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(contents.len().to_be_bytes());
        digest.update(contents);
    }
    format!("{:x}", digest.finalize())
}

fn skill_path(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
        || path == Path::new(INSTALLER_MARKER)
    {
        return Err(eyre!("Invalid embedded skill asset path: {name}"));
    }
    Ok(path.to_path_buf())
}

fn sibling_path(destination: &Path, purpose: &str) -> PathBuf {
    let parent = destination.parent().expect("destination has parent");
    let name = destination.file_name().expect("destination has name").to_string_lossy();
    parent.join(format!(".{name}.{purpose}.{}", Uuid::new_v4()))
}

fn home_dir() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn embedded(files: &[(&str, &str)]) -> EmbeddedSkill {
        EmbeddedSkill::from_files(
            files
                .iter()
                .map(|(path, contents)| (PathBuf::from(path), contents.as_bytes().to_vec()))
                .collect(),
        )
        .expect("embedded skill")
    }

    fn environment(home: &Path) -> SkillEnvironment {
        SkillEnvironment {
            home: Some(home.to_path_buf()),
            ..SkillEnvironment::default()
        }
    }

    #[test]
    fn embedded_skill_has_canonical_assets_but_no_scripts() {
        let skill = EmbeddedSkill::current().expect("embedded skill");

        assert!(skill.files().contains_key(Path::new("SKILL.md")));
        assert!(skill.files().keys().any(|path| path.starts_with("references")));
        assert!(skill.files().keys().any(|path| path.starts_with("agents")));
        assert!(skill.files().keys().all(|path| !path.starts_with("scripts")));
    }

    #[test]
    fn digest_is_deterministic_across_input_order() {
        let first = embedded(&[("SKILL.md", "skill"), ("references/cli.md", "reference")]);
        let second = embedded(&[("references/cli.md", "reference"), ("SKILL.md", "skill")]);

        assert_eq!(first.digest(), second.digest());
    }

    #[test]
    fn target_paths_follow_documented_user_scopes_and_overrides() {
        let root = tempdir().expect("temp dir");
        let environment = environment(root.path());

        assert_eq!(
            SkillTarget::Claude.destination(&environment).expect("claude path"),
            root.path().join(".claude/skills/esdiag")
        );
        assert_eq!(
            SkillTarget::Codex.destination(&environment).expect("codex path"),
            root.path().join(".agents/skills/esdiag")
        );
        assert_eq!(
            SkillTarget::OpenCode.destination(&environment).expect("opencode path"),
            root.path().join(".config/opencode/skills/esdiag")
        );

        let overrides = SkillEnvironment {
            claude_config_dir: Some(root.path().join("claude-config")),
            opencode_config_dir: Some(root.path().join("opencode-config")),
            ..environment
        };
        assert_eq!(
            SkillTarget::Claude.destination(&overrides).expect("claude override"),
            root.path().join("claude-config/skills/esdiag")
        );
        assert_eq!(
            SkillTarget::OpenCode
                .destination(&overrides)
                .expect("opencode override"),
            root.path().join("opencode-config/skills/esdiag")
        );
    }

    #[test]
    fn detection_requires_a_positive_existing_agent_home() {
        let root = tempdir().expect("temp dir");
        let environment = environment(root.path());
        assert!(!SkillTarget::Claude.detected(&environment));
        assert!(!SkillTarget::Codex.detected(&environment));
        assert!(!SkillTarget::OpenCode.detected(&environment));

        fs::create_dir_all(root.path().join(".claude")).expect("claude home");
        fs::create_dir_all(root.path().join(".codex")).expect("codex home");
        fs::create_dir_all(root.path().join(".config/opencode")).expect("opencode home");
        assert!(SkillTarget::Claude.detected(&environment));
        assert!(SkillTarget::Codex.detected(&environment));
        assert!(SkillTarget::OpenCode.detected(&environment));
    }

    #[test]
    fn preflight_classifies_missing_exact_managed_modified_and_unrecognized() {
        let root = tempdir().expect("temp dir");
        let destination = root.path().join("skills/esdiag");
        let current = embedded(&[("SKILL.md", "current")]);
        assert_eq!(inspect(&destination, &current).expect("missing"), SkillState::Missing);

        install(&destination, &current, false).expect("install");
        assert_eq!(inspect(&destination, &current).expect("exact"), SkillState::Exact);

        let older = embedded(&[("SKILL.md", "older")]);
        fs::remove_dir_all(&destination).expect("remove current");
        install(&destination, &older, false).expect("old install");
        assert!(matches!(
            inspect(&destination, &current).expect("managed"),
            SkillState::ManagedIntact { .. }
        ));

        fs::write(destination.join("SKILL.md"), "locally changed").expect("modify skill");
        assert_eq!(inspect(&destination, &current).expect("modified"), SkillState::Modified);

        fs::remove_file(destination.join(INSTALLER_MARKER)).expect("remove marker");
        assert_eq!(
            inspect(&destination, &current).expect("unrecognized"),
            SkillState::Unrecognized
        );
    }

    #[test]
    fn installation_preserves_conflicts_until_explicit_force() {
        let root = tempdir().expect("temp dir");
        let destination = root.path().join("skills/esdiag");
        fs::create_dir_all(&destination).expect("destination");
        fs::write(destination.join("SKILL.md"), "custom").expect("custom skill");
        let current = embedded(&[("SKILL.md", "current")]);

        assert_eq!(
            install(&destination, &current, false).expect("conflict").action,
            SkillAction::Conflict
        );
        assert_eq!(
            fs::read_to_string(destination.join("SKILL.md")).expect("custom remains"),
            "custom"
        );

        assert_eq!(
            install(&destination, &current, true).expect("force update").action,
            SkillAction::ForceUpdated
        );
        assert_eq!(
            fs::read_to_string(destination.join("SKILL.md")).expect("installed"),
            "current"
        );
        assert_eq!(inspect(&destination, &current).expect("exact"), SkillState::Exact);
    }

    #[test]
    fn managed_installation_updates_and_exact_installation_is_unchanged() {
        let root = tempdir().expect("temp dir");
        let destination = root.path().join("skills/esdiag");
        let older = embedded(&[("SKILL.md", "older")]);
        let current = embedded(&[("SKILL.md", "current")]);
        install(&destination, &older, false).expect("old install");

        assert_eq!(
            install(&destination, &current, false).expect("update").action,
            SkillAction::Updated
        );
        assert_eq!(
            install(&destination, &current, false).expect("unchanged").action,
            SkillAction::Unchanged
        );
    }

    #[test]
    fn embedded_paths_cannot_escape_a_skill_directory() {
        assert!(skill_path("../SKILL.md").is_err());
        assert!(skill_path(INSTALLER_MARKER).is_err());
    }
}
