// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Shell completion script generation and install-path helpers for `esdiag`.

use clap::Command;
use clap_complete::{
    generate,
    shells::{Bash, Fish, PowerShell, Zsh},
};
use eyre::{Result, eyre};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Shells that `esdiag setup completions` can emit scripts for.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
#[value(rename_all = "lower")]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

impl CompletionShell {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Powershell => "powershell",
        }
    }
}

/// Write a completion script for `shell` to `out` using the given clap command.
///
/// Generation is buffered first so a short-lived pipe consumer (for example
/// `head`) cannot panic clap_complete's fish writer on a broken pipe.
pub fn write_completion_script(shell: CompletionShell, cmd: &mut Command, out: &mut impl Write) -> Result<()> {
    let bin_name = cmd.get_name().to_string();
    let mut buffer = Vec::new();
    match shell {
        CompletionShell::Bash => generate(Bash, cmd, &bin_name, &mut buffer),
        CompletionShell::Zsh => generate(Zsh, cmd, &bin_name, &mut buffer),
        CompletionShell::Fish => generate(Fish, cmd, &bin_name, &mut buffer),
        CompletionShell::Powershell => generate(PowerShell, cmd, &bin_name, &mut buffer),
    }
    match out.write_all(&buffer) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Conventional install path for a completion script under `home`.
///
/// Bash honors absolute `XDG_DATA_HOME`; fish honors absolute `XDG_CONFIG_HOME`.
pub fn install_path(shell: CompletionShell, home: &Path) -> PathBuf {
    install_path_with_xdg(
        shell,
        home,
        absolute_env_path("XDG_DATA_HOME"),
        absolute_env_path("XDG_CONFIG_HOME"),
    )
}

fn install_path_with_xdg(
    shell: CompletionShell,
    home: &Path,
    xdg_data_home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
) -> PathBuf {
    match shell {
        CompletionShell::Bash => {
            let data = xdg_data_home.unwrap_or_else(|| home.join(".local/share"));
            data.join("bash-completion/completions").join("esdiag")
        }
        CompletionShell::Zsh => home.join(".zfunc").join("_esdiag"),
        CompletionShell::Fish => {
            let config = xdg_config_home.unwrap_or_else(|| home.join(".config"));
            config.join("fish/completions").join("esdiag.fish")
        }
        CompletionShell::Powershell => home.join(".config/powershell/Completions").join("esdiag.ps1"),
    }
}

fn absolute_env_path(var: &str) -> Option<PathBuf> {
    let value = std::env::var_os(var).filter(|value| !value.is_empty())?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// Quote a path for safe inclusion in a zsh single-quoted word.
fn shell_single_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

/// Quote a path for safe inclusion in a PowerShell single-quoted string.
fn powershell_single_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "''"))
}

/// One-line (or short) instruction the user must apply; never edits shell rc files.
pub fn install_instruction(shell: CompletionShell, path: &Path) -> String {
    match shell {
        CompletionShell::Bash => {
            let quoted = shell_single_quote(path);
            format!(
                "Requires bash-completion. With it enabled, new shells load {quoted}; otherwise source that file from ~/.bashrc."
            )
        }
        CompletionShell::Zsh => {
            let dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("~/.zfunc"));
            let quoted_dir = shell_single_quote(&dir);
            format!("Add to ~/.zshrc if missing: fpath=({quoted_dir} $fpath); autoload -Uz compinit; compinit")
        }
        CompletionShell::Fish => {
            let quoted = shell_single_quote(path);
            format!("Open a new fish shell; fish loads {quoted} from its completions directory.")
        }
        CompletionShell::Powershell => {
            let quoted = powershell_single_quote(path);
            format!("PowerShell does not autoload this path. Add to your profile if missing: . {quoted}")
        }
    }
}

/// Write the completion script to the conventional path under `home`.
///
/// Returns the written path and the follow-up instruction. Does not edit shell
/// startup files.
pub fn install_completion(shell: CompletionShell, cmd: &mut Command, home: &Path) -> Result<(PathBuf, String)> {
    let path = install_path(shell, home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&path)?;
    write_completion_script(shell, cmd, &mut file)?;
    file.flush()?;
    let instruction = install_instruction(shell, &path);
    Ok((path, instruction))
}

/// Resolve the user home directory for completion install paths.
pub fn user_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    Err(eyre!(
        "Unable to determine home directory for completion install (HOME or USERPROFILE)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Command, CommandFactory, Parser, Subcommand};
    use tempfile::tempdir;

    #[derive(Debug, Parser)]
    #[command(name = "esdiag")]
    struct SampleCli {
        #[command(subcommand)]
        command: SampleCommands,
    }

    #[derive(Debug, Subcommand)]
    enum SampleCommands {
        Collect,
        Process,
    }

    fn sample_command() -> Command {
        SampleCli::command()
    }

    #[test]
    fn install_paths_use_home_and_shell_conventions() {
        let home = Path::new("/tmp/esdiag-home");
        assert_eq!(
            install_path_with_xdg(CompletionShell::Bash, home, None, None),
            PathBuf::from("/tmp/esdiag-home/.local/share/bash-completion/completions/esdiag")
        );
        assert_eq!(
            install_path_with_xdg(CompletionShell::Zsh, home, None, None),
            PathBuf::from("/tmp/esdiag-home/.zfunc/_esdiag")
        );
        assert_eq!(
            install_path_with_xdg(CompletionShell::Fish, home, None, None),
            PathBuf::from("/tmp/esdiag-home/.config/fish/completions/esdiag.fish")
        );
        assert_eq!(
            install_path_with_xdg(CompletionShell::Powershell, home, None, None),
            PathBuf::from("/tmp/esdiag-home/.config/powershell/Completions/esdiag.ps1")
        );
    }

    #[test]
    fn install_paths_honor_xdg_data_and_config_home() {
        let home = Path::new("/tmp/esdiag-home");
        assert_eq!(
            install_path_with_xdg(CompletionShell::Bash, home, Some(PathBuf::from("/custom/data")), None),
            PathBuf::from("/custom/data/bash-completion/completions/esdiag")
        );
        assert_eq!(
            install_path_with_xdg(CompletionShell::Fish, home, None, Some(PathBuf::from("/custom/config"))),
            PathBuf::from("/custom/config/fish/completions/esdiag.fish")
        );
        // zsh and powershell remain under home even when XDG vars are set
        assert_eq!(
            install_path_with_xdg(
                CompletionShell::Zsh,
                home,
                Some(PathBuf::from("/custom/data")),
                Some(PathBuf::from("/custom/config"))
            ),
            PathBuf::from("/tmp/esdiag-home/.zfunc/_esdiag")
        );
    }

    #[test]
    fn generated_scripts_name_binary_and_sample_commands() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
            CompletionShell::Powershell,
        ] {
            let mut cmd = sample_command();
            let mut buffer = Vec::new();
            write_completion_script(shell, &mut cmd, &mut buffer).unwrap();
            let script = String::from_utf8(buffer).unwrap();
            assert!(
                script.contains("esdiag"),
                "{} script missing binary name",
                shell.as_str()
            );
            assert!(script.contains("collect"), "{} script missing collect", shell.as_str());
            assert!(script.contains("process"), "{} script missing process", shell.as_str());
            assert!(
                !script.trim_start().starts_with("result:"),
                "{} script looks like YAML",
                shell.as_str()
            );
        }
    }

    #[test]
    fn install_writes_script_and_returns_instruction() {
        let dir = tempdir().unwrap();
        let mut cmd = sample_command();
        let (path, instruction) = install_completion(CompletionShell::Zsh, &mut cmd, dir.path()).unwrap();
        assert_eq!(path, dir.path().join(".zfunc/_esdiag"));
        assert!(path.is_file());
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("esdiag"));
        assert!(!instruction.is_empty());
        assert!(instruction.contains("fpath"));

        let bash_instruction = install_instruction(
            CompletionShell::Bash,
            &install_path_with_xdg(CompletionShell::Bash, dir.path(), None, None),
        );
        assert!(bash_instruction.contains("bash-completion"));
        let pwsh_instruction = install_instruction(
            CompletionShell::Powershell,
            &install_path_with_xdg(CompletionShell::Powershell, dir.path(), None, None),
        );
        assert!(pwsh_instruction.contains("does not autoload"));
    }

    #[test]
    fn zsh_instruction_quotes_paths_with_spaces_and_apostrophes() {
        let path = Path::new("/tmp/user's dir/.zfunc/_esdiag");
        let instruction = install_instruction(CompletionShell::Zsh, path);
        assert!(instruction.contains("fpath=('/tmp/user'\\''s dir/.zfunc' $fpath)"));
    }

    #[test]
    fn powershell_instruction_escapes_apostrophes_in_path() {
        let path = Path::new("/tmp/user's Completions/esdiag.ps1");
        let instruction = install_instruction(CompletionShell::Powershell, path);
        assert!(instruction.contains(". '/tmp/user''s Completions/esdiag.ps1'"));
    }

    #[test]
    fn install_instructions_are_non_empty_for_each_shell() {
        let home = Path::new("/tmp/home");
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
            CompletionShell::Powershell,
        ] {
            let path = install_path_with_xdg(shell, home, None, None);
            let instruction = install_instruction(shell, &path);
            assert!(!instruction.is_empty(), "empty instruction for {}", shell.as_str());
        }
    }
}
