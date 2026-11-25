// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::catalog::{SyscallSource, option_key};
use crate::processor::{ProcessorSummary, elasticsearch::missing_source_error};
use crate::receiver::Receiver;
use eyre::Result;
use futures::stream::{self, BoxStream, StreamExt};

/// How a selected syscall source entered the processing selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallMode {
    /// Selected by diagnostic-type defaults, or no selection was given.
    Default,
    /// Listed in the user-supplied selection.
    Requested,
}

impl SyscallMode {
    pub fn for_option(option: &str, requested: &[String]) -> Self {
        if requested.iter().any(|key| key == option) {
            Self::Requested
        } else {
            Self::Default
        }
    }
}

/// A syscall file's lines, starting at its first non-blank line.
pub type SyscallLines = BoxStream<'static, Result<String>>;

/// The outcome of opening a selected syscall source.
pub enum SyscallInput {
    /// The file exists and has at least one non-blank line.
    Lines(SyscallLines),
    /// The source is missing; the summary already follows the missing-file policy.
    Missing(ProcessorSummary),
    /// The file exists but could not be read; the summary carries one warning.
    Unreadable(ProcessorSummary),
}

/// Decide whether a selected syscall source is missing before its processor
/// runs, applying one missing-file policy to every syscall processor: missing
/// in default mode is silent, missing when explicitly requested is a warning,
/// and an existing file that cannot be read is always a warning.
pub async fn open_syscall(receiver: &Receiver, key: &str, source: &SyscallSource, mode: SyscallMode) -> SyscallInput {
    let option = option_key(key);
    if !receiver.is_bundle() {
        return SyscallInput::Missing(missing(
            &option,
            mode,
            format!(
                "{} is unavailable: the diagnostic was not loaded from a bundle",
                source.filename
            ),
        ));
    }

    let mut lines = match receiver.read_bundle_lines(&source.filename).await {
        Ok(lines) => lines,
        Err(err) if missing_source_error(&err) => {
            tracing::debug!("{} is absent: {}", source.filename, err);
            return SyscallInput::Missing(missing(
                &option,
                mode,
                format!("{} was requested but is not in the bundle", source.filename),
            ));
        }
        Err(err) => return SyscallInput::Unreadable(unreadable(&option, &source.filename, &err)),
    };

    while let Some(line) = lines.next().await {
        match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => return SyscallInput::Lines(stream::once(async { Ok(line) }).chain(lines).boxed()),
            Err(err) => return SyscallInput::Unreadable(unreadable(&option, &source.filename, &err)),
        }
    }

    SyscallInput::Missing(missing(
        &option,
        mode,
        format!("{} was requested but contains no data", source.filename),
    ))
}

fn missing(option: &str, mode: SyscallMode, reason: String) -> ProcessorSummary {
    let summary = ProcessorSummary::missing(option.to_string());
    match mode {
        SyscallMode::Default => summary,
        SyscallMode::Requested => summary.with_warning(reason),
    }
}

fn unreadable(option: &str, filename: &str, err: &eyre::Report) -> ProcessorSummary {
    ProcessorSummary::new(option.to_string())
        .was_parsed()
        .with_warning(format!("{filename} could not be read: {err:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Uri;
    use crate::processor::EventSeverity;
    use std::fs;

    fn sar_source() -> SyscallSource {
        super::super::catalog::syscall_source("sar")
            .cloned()
            .expect("sar entry")
    }

    fn directory_receiver(files: &[(&str, &str)]) -> (tempfile::TempDir, Receiver) {
        let dir = tempfile::tempdir().expect("bundle directory");
        for (path, contents) in files {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
            fs::write(path, contents).expect("write file");
        }
        let receiver = Receiver::try_from(Uri::Directory(dir.path().to_path_buf())).expect("receiver");
        (dir, receiver)
    }

    fn warnings(summary: &ProcessorSummary) -> Vec<String> {
        summary
            .events()
            .iter()
            .filter(|event| event.severity == EventSeverity::Warning)
            .map(|event| event.reason.clone())
            .collect()
    }

    #[test]
    fn mode_is_requested_only_for_listed_options() {
        let requested = vec!["syscalls.sar".to_string()];
        assert_eq!(
            SyscallMode::for_option("syscalls.sar", &requested),
            SyscallMode::Requested
        );
        assert_eq!(SyscallMode::for_option("syscalls.sar", &[]), SyscallMode::Default);
    }

    #[tokio::test]
    async fn absent_file_is_silent_by_default_and_warns_when_requested() {
        let (_dir, receiver) = directory_receiver(&[("syscalls/ss.txt", "LISTEN\n")]);

        let SyscallInput::Missing(summary) = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Default).await
        else {
            panic!("absent sar.txt is missing");
        };
        assert!(summary.source.missing);
        assert!(warnings(&summary).is_empty());

        let SyscallInput::Missing(summary) =
            open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Requested).await
        else {
            panic!("absent sar.txt is missing");
        };
        assert!(summary.source.missing);
        assert_eq!(summary.processor, "syscalls.sar");
        let warnings = warnings(&summary);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("syscalls/sar.txt"), "{warnings:?}");
    }

    #[tokio::test]
    async fn api_receivers_are_missing_and_warn_only_when_requested() {
        let host = crate::data::KnownHostBuilder::new(url::Url::parse("https://es.example.com:9200").expect("url"))
            .auth(crate::data::Auth::apikey("test-api-key"))
            .build()
            .expect("host");
        let receiver = Receiver::try_from(host).expect("API receiver");

        let input = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Default).await;
        assert!(matches!(&input, SyscallInput::Missing(summary) if warnings(summary).is_empty()));
        let input = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Requested).await;
        assert!(matches!(&input, SyscallInput::Missing(summary) if warnings(summary).len() == 1));
    }

    #[tokio::test]
    async fn bundle_without_syscalls_directory_is_missing() {
        let (_dir, receiver) = directory_receiver(&[("version.json", "{}")]);
        let input = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Default).await;
        assert!(matches!(&input, SyscallInput::Missing(summary) if warnings(summary).is_empty()));
    }

    #[tokio::test]
    async fn blank_file_is_missing() {
        let (_dir, receiver) = directory_receiver(&[("syscalls/sar.txt", "\n   \n\t\n")]);

        let input = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Default).await;
        assert!(matches!(&input, SyscallInput::Missing(summary) if warnings(summary).is_empty()));

        let input = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Requested).await;
        assert!(matches!(&input, SyscallInput::Missing(summary) if warnings(summary).len() == 1));
    }

    #[tokio::test]
    async fn lines_start_at_the_first_non_blank_line_and_keep_the_rest() {
        let (_dir, receiver) = directory_receiver(&[("syscalls/sar.txt", "\n\nLinux header\n\nrow\n")]);

        let SyscallInput::Lines(lines) = open_syscall(&receiver, "sar", &sar_source(), SyscallMode::Default).await
        else {
            panic!("non-blank file yields lines");
        };
        let lines: Vec<String> = lines.map(|line| line.expect("line")).collect().await;
        assert_eq!(lines, ["Linux header", "", "row"]);
    }
}
