// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use chrono::{Duration, NaiveDate, Utc};
use esdiag_core::data::{daily_log_filename, open_daily_log_file};
use eyre::Result;
use flate2::{Compression, write::GzEncoder};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;
use uuid::Uuid;

type DateClock = Arc<dyn Fn() -> NaiveDate + Send + Sync>;

struct WriterInner {
    current_date: NaiveDate,
    file: File,
}

/// Daily-rotating file writer for tracing output (`esdiag-YYYY-MM-DD.log`).
///
/// - Creates a new file when the UTC calendar day changes.
/// - Never compresses or scans the directory on startup.
/// - On daily rollover, spawns background retention pruning and (when
///   `compress_on_rollover` is enabled, i.e. `serve` mode) background gzip
///   compression of previous days' `.log` files.
/// - On process exit, [`DailyLogMaintenance::run_on_exit`] flushes the active
///   file, compresses previous days' `.log` files, and prunes expired files.
#[derive(Clone)]
pub struct DailyLogWriter {
    dir: PathBuf,
    retention_days: u32,
    compress_on_rollover: bool,
    clock: DateClock,
    inner: Arc<Mutex<WriterInner>>,
}

/// Handle retained by [`super::LogsGuard`] to run exit maintenance for file logs.
pub struct DailyLogMaintenance {
    writer: DailyLogWriter,
}

impl DailyLogWriter {
    /// Opens today's daily log file in `dir` without performing any directory
    /// scan or compression on startup.
    pub fn open(dir: PathBuf, retention_days: u32, compress_on_rollover: bool) -> Result<(Self, DailyLogMaintenance)> {
        Self::open_with_clock(
            dir,
            retention_days,
            compress_on_rollover,
            Arc::new(|| Utc::now().date_naive()),
        )
    }

    pub(crate) fn open_with_clock(
        dir: PathBuf,
        retention_days: u32,
        compress_on_rollover: bool,
        clock: DateClock,
    ) -> Result<(Self, DailyLogMaintenance)> {
        let today = (clock)();
        let file = open_daily_log_file(&dir, today)?;
        let writer = Self {
            dir,
            retention_days,
            compress_on_rollover,
            clock,
            inner: Arc::new(Mutex::new(WriterInner {
                current_date: today,
                file,
            })),
        };
        let maintenance = DailyLogMaintenance { writer: writer.clone() };
        Ok((writer, maintenance))
    }

    fn flush_active(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            let _ = guard.file.flush();
        }
    }

    fn write_bytes(&self, buf: &[u8]) -> io::Result<usize> {
        let (written, rolled_over_date) = {
            let mut guard = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let today = (self.clock)();
            let mut rolled_over_date = None;
            if today > guard.current_date {
                let _ = guard.file.flush();
                match open_daily_log_file(&self.dir, today) {
                    Ok(new_file) => {
                        guard.file = new_file;
                        guard.current_date = today;
                        rolled_over_date = Some(today);
                    }
                    Err(err) => {
                        eprintln!(
                            "Failed to rotate daily log file to {}: {err}",
                            daily_log_filename(today)
                        );
                    }
                }
            }
            (guard.file.write(buf)?, rolled_over_date)
        };

        if let Some(today) = rolled_over_date {
            let dir = self.dir.clone();
            let retention_days = self.retention_days;
            let compress = self.compress_on_rollover;
            let maintenance = move || {
                prune_expired_logs(&dir, today, retention_days);
                if compress {
                    compress_previous_logs(&dir, today);
                }
            };
            // Always run rollover maintenance on a detached OS thread so
            // logging calls from any thread (with or without an entered Tokio
            // runtime) never block on directory scans or gzip compression, and
            // Tokio runtime shutdown cannot wait on stalled filesystem I/O.
            if let Err(err) = std::thread::Builder::new()
                .name("esdiag-log-rollover".into())
                .spawn(maintenance)
            {
                eprintln!("Failed to spawn file log rollover maintenance thread: {err}");
            }
        }

        Ok(written)
    }
}

pub struct DailyLogGuard<'a> {
    writer: &'a DailyLogWriter,
}

impl Write for DailyLogGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write_bytes(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush_active();
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for DailyLogWriter {
    type Writer = DailyLogGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        DailyLogGuard { writer: self }
    }
}

impl DailyLogMaintenance {
    /// Flushes the active daily log file, prunes expired files, and gzips
    /// previous days' `.log` files.
    pub fn run_on_exit(&self) {
        let never_cancelled = AtomicBool::new(false);
        self.run_on_exit_with_cancel(&never_cancelled);
    }

    /// Same as [`Self::run_on_exit`], stopping early between files and copy
    /// chunks when `cancelled` becomes `true`.
    pub fn run_on_exit_with_cancel(&self, cancelled: &AtomicBool) {
        let today = (self.writer.clock)();
        // Rotate off any prior day's file before running maintenance so our own
        // shared advisory lock does not block compressing yesterday's log.
        if let Ok(mut guard) = self.writer.inner.lock() {
            let _ = guard.file.flush();
            if guard.current_date < today
                && let Ok(new_file) = open_daily_log_file(&self.writer.dir, today)
            {
                guard.file = new_file;
                guard.current_date = today;
            }
        }
        prune_expired_logs_with_cancel(&self.writer.dir, today, self.writer.retention_days, cancelled);
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        compress_previous_logs_with_cancel(&self.writer.dir, today, cancelled);
    }
}

/// Parses `esdiag-YYYY-MM-DD.log` or `esdiag-YYYY-MM-DD.log.gz`, returning
/// `(date, is_compressed)`.
pub(crate) fn parse_daily_log_name(file_name: &str) -> Option<(NaiveDate, bool)> {
    let rest = file_name.strip_prefix("esdiag-")?;
    if let Some(date_str) = rest.strip_suffix(".log.gz") {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
        Some((date, true))
    } else if let Some(date_str) = rest.strip_suffix(".log") {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
        Some((date, false))
    } else {
        None
    }
}

/// Parses an abandoned Windows staging filename `esdiag-YYYY-MM-DD.log.<id>.compressing`,
/// returning its log date.
pub(crate) fn parse_staged_daily_log_name(file_name: &str) -> Option<NaiveDate> {
    let rest = file_name.strip_prefix("esdiag-")?;
    let without_suffix = rest.strip_suffix(".compressing")?;
    let (date_str, _) = without_suffix.split_once(".log.")?;
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}

/// Parses a Windows collision-recovery archive `esdiag-YYYY-MM-DD.log.<id>.gz`,
/// returning its log date.
pub(crate) fn parse_recovery_archive_name(file_name: &str) -> Option<NaiveDate> {
    let rest = file_name.strip_prefix("esdiag-")?;
    let without_suffix = rest.strip_suffix(".gz")?;
    let (date_str, id) = without_suffix.split_once(".log.")?;
    if id.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}

/// Parses an abandoned compression temporary file `esdiag-YYYY-MM-DD.log.gz.<id>.tmp`,
/// returning its log date.
pub(crate) fn parse_temp_archive_name(file_name: &str) -> Option<NaiveDate> {
    let rest = file_name.strip_prefix("esdiag-")?;
    let without_suffix = rest.strip_suffix(".tmp")?;
    let (date_str, id) = without_suffix.split_once(".log.gz.")?;
    if id.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}
/// Compresses all uncompressed `esdiag-YYYY-MM-DD.log` files in `dir` with
/// `date < today` into `esdiag-YYYY-MM-DD.log.gz` using atomic temp-file rename
/// and `0600` permissions on Unix. Safe under concurrent processes.
pub(crate) fn compress_previous_logs(dir: &Path, today: NaiveDate) {
    let never_cancelled = AtomicBool::new(false);
    compress_previous_logs_with_cancel(dir, today, &never_cancelled);
}

fn compress_previous_logs_with_cancel(dir: &Path, today: NaiveDate, cancelled: &AtomicBool) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("Failed to read logs directory '{}': {err}", dir.display());
            return;
        }
    };

    for entry in entries.flatten() {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if let Some((date, is_compressed)) = parse_daily_log_name(name_str) {
            if is_compressed || date >= today {
                continue;
            }
            let src_path = entry.path();
            let gz_path = dir.join(format!("{name_str}.gz"));
            if gz_path.exists() {
                continue;
            }
            if let Err(err) = compress_single_log(&src_path, &gz_path, cancelled) {
                eprintln!("Failed to compress log file '{}': {err}", src_path.display());
            }
            continue;
        }
        #[cfg(windows)]
        if let Some(date) = parse_staged_daily_log_name(name_str) {
            if date >= today {
                continue;
            }
            let staging_path = entry.path();
            let src_path = dir.join(daily_log_filename(date));
            let gz_path = dir.join(format!("{}.gz", daily_log_filename(date)));
            if let Err(err) = compress_staged_log(&staging_path, &src_path, &gz_path, cancelled) {
                eprintln!("Failed to recover staged log file '{}': {err}", staging_path.display());
            }
        }
    }
}

#[cfg(not(windows))]
fn try_open_exclusive_for_maintenance(path: &Path) -> io::Result<Option<File>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for _ in 0..8 {
            let file = match File::open(path) {
                Ok(file) => file,
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(err) => return Err(err),
            };
            match file.try_lock() {
                Ok(()) => {
                    let fd_meta = file.metadata()?;
                    let path_meta = match fs::metadata(path) {
                        Ok(meta) => meta,
                        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
                        Err(err) => return Err(err),
                    };
                    if fd_meta.dev() == path_meta.dev() && fd_meta.ino() == path_meta.ino() {
                        return Ok(Some(file));
                    }
                    // Path was replaced between open and lock; drop the stale
                    // handle and retry against the current pathname.
                }
                Err(fs::TryLockError::WouldBlock) => return Ok(None),
                Err(fs::TryLockError::Error(err)) => return Err(err),
            }
        }
        Ok(None)
    }
    #[cfg(not(unix))]
    {
        match File::open(path) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(windows)]
fn restore_staged_log(staging_path: &Path, src_path: &Path) {
    // `hard_link` fails without overwriting if a writer created a new `src_path`.
    if fs::hard_link(staging_path, src_path).is_ok() {
        let _ = fs::remove_file(staging_path);
    }
}

#[cfg(windows)]
fn compress_single_log(src_path: &Path, gz_path: &Path, cancelled: &AtomicBool) -> io::Result<()> {
    let staging_name = format!(
        "{}.{}-{}.compressing",
        src_path.file_name().and_then(|n| n.to_str()).unwrap_or("esdiag.log"),
        std::process::id(),
        Uuid::new_v4()
    );
    let staging_path = src_path.parent().unwrap_or_else(|| Path::new(".")).join(staging_name);

    // Atomically detach `src_path` before reading it. Active writers open
    // daily logs without `FILE_SHARE_DELETE`, so this fails with
    // `ERROR_SHARING_VIOLATION` (32) whenever any writer still holds the file
    // and eliminates any race between publishing `.gz` and deleting the source.
    match fs::rename(src_path, &staging_path) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) if err.raw_os_error() == Some(32) => return Ok(()),
        Err(err) => return Err(err),
    }

    compress_staged_log(&staging_path, src_path, gz_path, cancelled)
}

#[cfg(windows)]
fn compress_staged_log(staging_path: &Path, src_path: &Path, gz_path: &Path, cancelled: &AtomicBool) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    // Claim exclusive ownership of the staging file (`share_mode(0)`) so
    // concurrent recovery/pruning scans skip actively compressed staging files
    // via `ERROR_SHARING_VIOLATION` (32), while abandoned staging files from an
    // interrupted exit are recovered on the next run.
    let mut input = match OpenOptions::new().read(true).share_mode(0).open(staging_path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) if err.raw_os_error() == Some(32) => return Ok(()),
        Err(err) => return Err(err),
    };

    let tmp_name = format!(
        "{}.{}-{}.tmp",
        gz_path.file_name().and_then(|n| n.to_str()).unwrap_or("esdiag.log.gz"),
        std::process::id(),
        Uuid::new_v4()
    );
    let tmp_path = gz_path.parent().unwrap_or_else(|| Path::new(".")).join(tmp_name);

    let mut was_cancelled = false;
    let compress_result = (|| -> io::Result<()> {
        // Open without `FILE_SHARE_DELETE` (`share_mode(0)`) so concurrent
        // maintenance scans skip active `.tmp` files via
        // `ERROR_SHARING_VIOLATION` (32), while abandoned `.tmp` files from an
        // interrupted exit are cleaned up on later runs.
        let tmp_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&tmp_path)?;
        let mut encoder = GzEncoder::new(tmp_file, Compression::default());
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancelled.load(Ordering::Relaxed) {
                was_cancelled = true;
                return Ok(());
            }
            let read = input.read(&mut buf)?;
            if read == 0 {
                break;
            }
            encoder.write_all(&buf[..read])?;
        }
        let file = encoder.finish()?;
        file.sync_all()?;
        Ok(())
    })();

    if was_cancelled {
        drop(input);
        let _ = fs::remove_file(&tmp_path);
        restore_staged_log(staging_path, src_path);
        return Ok(());
    }

    if let Err(err) = compress_result {
        drop(input);
        let _ = fs::remove_file(&tmp_path);
        restore_staged_log(staging_path, src_path);
        return Err(err);
    }

    // Publish atomically without overwriting an existing archive: `hard_link`
    // fails with `AlreadyExists` if another process already published
    // `gz_path`, in which case we publish under a unique recovery archive name
    // so no staged log entries are lost.
    let publish_result = match fs::hard_link(&tmp_path, gz_path) {
        Ok(()) => {
            let _ = fs::remove_file(&tmp_path);
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let recovery_name = format!(
                "{}.{}-{}.gz",
                src_path.file_name().and_then(|n| n.to_str()).unwrap_or("esdiag.log"),
                std::process::id(),
                Uuid::new_v4()
            );
            let recovery_path = gz_path.parent().unwrap_or_else(|| Path::new(".")).join(recovery_name);
            fs::rename(&tmp_path, &recovery_path)
        }
        Err(err) => Err(err),
    };
    if let Err(err) = publish_result {
        drop(input);
        let _ = fs::remove_file(&tmp_path);
        restore_staged_log(staging_path, src_path);
        return Err(err);
    }
    drop(input);
    match fs::remove_file(staging_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) if err.raw_os_error() == Some(32) => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(not(windows))]
fn compress_single_log(src_path: &Path, gz_path: &Path, cancelled: &AtomicBool) -> io::Result<()> {
    let Some(mut input) = try_open_exclusive_for_maintenance(src_path)? else {
        return Ok(());
    };

    let tmp_name = format!(
        "{}.{}-{}.tmp",
        gz_path.file_name().and_then(|n| n.to_str()).unwrap_or("esdiag.log.gz"),
        std::process::id(),
        Uuid::new_v4()
    );
    let tmp_path = gz_path.parent().unwrap_or_else(|| Path::new(".")).join(tmp_name);

    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let tmp_file = options.write(true).create_new(true).open(&tmp_path)?;
    // Hold an exclusive advisory lock on the temporary file while compressing
    // so concurrent maintenance scans recognize active `.tmp` files and only
    // delete abandoned ones left behind by an interrupted exit.
    #[cfg(unix)]
    tmp_file.lock()?;

    let mut was_cancelled = false;
    let compress_result = (|| -> io::Result<()> {
        let mut encoder = GzEncoder::new(tmp_file, Compression::default());
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancelled.load(Ordering::Relaxed) {
                was_cancelled = true;
                return Ok(());
            }
            let read = input.read(&mut buf)?;
            if read == 0 {
                break;
            }
            encoder.write_all(&buf[..read])?;
        }
        let file = encoder.finish()?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();

    if was_cancelled {
        let _ = fs::remove_file(&tmp_path);
        return Ok(());
    }

    if let Err(err) = compress_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    if gz_path.exists() {
        let _ = fs::remove_file(&tmp_path);
        return Ok(());
    }

    if let Err(err) = fs::rename(&tmp_path, gz_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    match fs::remove_file(src_path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Deletes `esdiag-YYYY-MM-DD.log` and `esdiag-YYYY-MM-DD.log.gz` files in
/// `dir` whose date is strictly older than `today - retention_days`.
pub(crate) fn prune_expired_logs(dir: &Path, today: NaiveDate, retention_days: u32) {
    let never_cancelled = AtomicBool::new(false);
    prune_expired_logs_with_cancel(dir, today, retention_days, &never_cancelled);
}

fn prune_expired_logs_with_cancel(dir: &Path, today: NaiveDate, retention_days: u32, cancelled: &AtomicBool) {
    let Some(cutoff) = today.checked_sub_signed(Duration::days(i64::from(retention_days))) else {
        return;
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!(
                "Failed to read logs directory '{}' for retention pruning: {err}",
                dir.display()
            );
            return;
        }
    };

    for entry in entries.flatten() {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if parse_temp_archive_name(name_str).is_some() {
            let path = entry.path();
            #[cfg(not(windows))]
            {
                let file = match try_open_exclusive_for_maintenance(&path) {
                    Ok(Some(file)) => file,
                    Ok(None) => continue,
                    Err(err) => {
                        eprintln!("Failed to lock abandoned temp archive '{}': {err}", path.display());
                        continue;
                    }
                };
                if let Err(err) = fs::remove_file(&path)
                    && err.kind() != io::ErrorKind::NotFound
                {
                    eprintln!("Failed to remove abandoned temp archive '{}': {err}", path.display());
                }
                drop(file);
            }
            #[cfg(windows)]
            {
                if let Err(err) = fs::remove_file(&path)
                    && err.kind() != io::ErrorKind::NotFound
                    && err.raw_os_error() != Some(32)
                {
                    eprintln!("Failed to remove abandoned temp archive '{}': {err}", path.display());
                }
            }
            continue;
        }
        let (date, is_compressed) = if let Some(parsed) = parse_daily_log_name(name_str) {
            parsed
        } else if let Some(date) =
            parse_staged_daily_log_name(name_str).or_else(|| parse_recovery_archive_name(name_str))
        {
            // Treat abandoned Windows `.compressing` staging files and
            // collision-recovery `.log.<id>.gz` archives as expired when older
            // than the retention window (active staging files are held with
            // `share_mode(0)` so `remove_file` skips them with error 32).
            (date, true)
        } else {
            continue;
        };
        if date >= cutoff {
            continue;
        }
        let path = entry.path();
        #[cfg(not(windows))]
        if !is_compressed {
            let file = match try_open_exclusive_for_maintenance(&path) {
                Ok(Some(file)) => file,
                Ok(None) => continue,
                Err(err) => {
                    eprintln!("Failed to lock expired log file '{}': {err}", path.display());
                    continue;
                }
            };
            // Keep the exclusive lock held across `remove_file` so writers
            // cannot race in between unlock and unlink.
            if let Err(err) = fs::remove_file(&path)
                && err.kind() != io::ErrorKind::NotFound
            {
                eprintln!("Failed to remove expired log file '{}': {err}", path.display());
            }
            drop(file);
            continue;
        }
        // On Windows, active daily logs are opened without `FILE_SHARE_DELETE`,
        // so `remove_file` atomically fails with `ERROR_SHARING_VIOLATION` (32)
        // while any writer is open.
        if let Err(err) = fs::remove_file(&path)
            && err.kind() != io::ErrorKind::NotFound
        {
            #[cfg(windows)]
            if err.raw_os_error() == Some(32) {
                continue;
            }
            eprintln!("Failed to remove expired log file '{}': {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DailyLogWriter, compress_previous_logs, prune_expired_logs};
    use chrono::NaiveDate;
    use flate2::read::GzDecoder;
    use std::fs;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn no_compression_on_startup_and_gzips_prior_days_on_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let prior = dir.join("esdiag-2026-11-23.log");
        fs::write(&prior, "yesterday log line\n").unwrap();

        let clock = Arc::new(Mutex::new(d(2026, 11, 24)));
        let clock_fn = {
            let clock = Arc::clone(&clock);
            Arc::new(move || *clock.lock().unwrap())
        };

        let (writer, maintenance) = DailyLogWriter::open_with_clock(dir.clone(), 30, false, clock_fn).unwrap();
        writer.make_writer().write_all(b"today log line\n").unwrap();

        // Startup must NOT compress prior day's file.
        assert!(prior.exists());
        assert!(!dir.join("esdiag-2026-11-23.log.gz").exists());
        assert!(dir.join("esdiag-2026-11-24.log").exists());

        // Exit maintenance gzips prior day and leaves today uncompressed.
        maintenance.run_on_exit();
        assert!(!prior.exists());
        let gz_path = dir.join("esdiag-2026-11-23.log.gz");
        assert!(gz_path.exists());
        assert!(dir.join("esdiag-2026-11-24.log").exists());

        let mut decoded = String::new();
        GzDecoder::new(fs::File::open(&gz_path).unwrap())
            .read_to_string(&mut decoded)
            .unwrap();
        assert_eq!(decoded, "yesterday log line\n");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&gz_path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn daily_rollover_creates_new_file_and_gzips_previous_in_serve_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let clock = Arc::new(Mutex::new(d(2026, 11, 24)));
        let clock_fn = {
            let clock = Arc::clone(&clock);
            Arc::new(move || *clock.lock().unwrap())
        };

        let (writer, _maintenance) = DailyLogWriter::open_with_clock(dir.clone(), 30, true, clock_fn).unwrap();
        writer.make_writer().write_all(b"day one\n").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("esdiag-2026-11-24.log")).unwrap(),
            "day one\n"
        );

        // Advance day and write again -> rotates to Nov 25 and gzips Nov 24.
        *clock.lock().unwrap() = d(2026, 11, 25);
        writer.make_writer().write_all(b"day two\n").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("esdiag-2026-11-25.log")).unwrap(),
            "day two\n"
        );
        // Rollover maintenance runs asynchronously on a detached OS thread;
        // wait briefly for Nov 24 compression to complete.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while (dir.join("esdiag-2026-11-24.log").exists() || !dir.join("esdiag-2026-11-24.log.gz").exists())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!dir.join("esdiag-2026-11-24.log").exists());
        assert!(dir.join("esdiag-2026-11-24.log.gz").exists());
    }
    #[test]
    fn retention_prunes_expired_logs_and_preserves_boundary_and_unrelated_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let expired_gz = dir.join("esdiag-2026-10-20.log.gz");
        let expired_log = dir.join("esdiag-2026-10-24.log");
        let boundary_gz = dir.join("esdiag-2026-10-25.log.gz");
        let unrelated = dir.join("notes.txt");

        for file in [&expired_gz, &expired_log, &boundary_gz, &unrelated] {
            fs::write(file, b"data").unwrap();
        }

        prune_expired_logs(dir, d(2026, 11, 24), 30);

        assert!(!expired_gz.exists(), "35 days old must be deleted");
        assert!(!expired_log.exists(), "31 days old must be deleted");
        assert!(boundary_gz.exists(), "30 days old must be preserved");
        assert!(unrelated.exists(), "unrelated files must never be deleted");
    }

    #[test]
    fn compress_skips_when_gz_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let log = dir.join("esdiag-2026-11-23.log");
        let gz = dir.join("esdiag-2026-11-23.log.gz");
        fs::write(&log, b"raw").unwrap();
        fs::write(&gz, b"existing-gz").unwrap();

        compress_previous_logs(dir, d(2026, 11, 24));
        assert_eq!(fs::read(&gz).unwrap(), b"existing-gz");
    }

    #[test]
    fn compress_skips_file_held_open_by_active_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        let yesterday = d(2026, 11, 23);
        let today = d(2026, 11, 24);

        let mut active_writer = esdiag_core::data::open_daily_log_file(&dir, yesterday).unwrap();
        active_writer.write_all(b"still writing\n").unwrap();

        compress_previous_logs(&dir, today);
        assert!(dir.join("esdiag-2026-11-23.log").exists());
        assert!(!dir.join("esdiag-2026-11-23.log.gz").exists());

        drop(active_writer);
        compress_previous_logs(&dir, today);
        assert!(!dir.join("esdiag-2026-11-23.log").exists());
        assert!(dir.join("esdiag-2026-11-23.log.gz").exists());
    }

    #[test]
    fn prune_expired_logs_does_not_panic_on_u32_max_retention() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let old = dir.join("esdiag-2026-10-20.log.gz");
        fs::write(&old, b"old").unwrap();

        prune_expired_logs(dir, d(2026, 11, 24), u32::MAX);
        assert!(
            old.exists(),
            "huge retention window must preserve files without panicking"
        );
    }

    #[test]
    fn maintenance_respects_cancellation_without_leaving_temp_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let log = dir.join("esdiag-2026-11-23.log");
        fs::write(&log, b"payload\n").unwrap();

        let cancelled = std::sync::atomic::AtomicBool::new(true);
        super::compress_previous_logs_with_cancel(dir, d(2026, 11, 24), &cancelled);
        assert!(log.exists(), "cancelled compression must preserve original log");
        assert!(!dir.join("esdiag-2026-11-23.log.gz").exists());

        let entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1, "cancelled compression must not leave .tmp files");
    }

    #[test]
    fn parse_staged_daily_log_name_and_prune_abandoned_staging_file() {
        let parsed = super::parse_staged_daily_log_name("esdiag-2026-10-20.log.1234-uuid.compressing");
        assert_eq!(parsed, Some(d(2026, 10, 20)));
        assert_eq!(super::parse_staged_daily_log_name("notes.compressing"), None);
        assert_eq!(
            super::parse_recovery_archive_name("esdiag-2026-10-20.log.1234-uuid.gz"),
            Some(d(2026, 10, 20))
        );
        assert_eq!(super::parse_recovery_archive_name("esdiag-2026-10-20.log.gz"), None);
        assert_eq!(
            super::parse_temp_archive_name("esdiag-2026-11-23.log.gz.1234-uuid.tmp"),
            Some(d(2026, 11, 23))
        );
        assert_eq!(super::parse_temp_archive_name("notes.tmp"), None);

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let expired_staged = dir.join("esdiag-2026-10-20.log.1234-uuid.compressing");
        let expired_recovery = dir.join("esdiag-2026-10-20.log.1234-uuid.gz");
        let recent_staged = dir.join("esdiag-2026-11-23.log.1234-uuid.compressing");
        let abandoned_tmp = dir.join("esdiag-2026-11-23.log.gz.1234-uuid.tmp");
        let active_tmp = dir.join("esdiag-2026-11-23.log.gz.5678-uuid.tmp");
        fs::write(&expired_staged, b"old").unwrap();
        fs::write(&expired_recovery, b"old-gz").unwrap();
        fs::write(&recent_staged, b"recent").unwrap();
        fs::write(&abandoned_tmp, b"partial").unwrap();

        // Simulate an active compressor holding an exclusive lock on `active_tmp`.
        let active_handle = {
            #[cfg(unix)]
            {
                let file = fs::File::create(&active_tmp).unwrap();
                file.lock().unwrap();
                file
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .share_mode(0)
                    .open(&active_tmp)
                    .unwrap()
            }
            #[cfg(not(any(unix, windows)))]
            {
                fs::File::create(&active_tmp).unwrap()
            }
        };

        prune_expired_logs(dir, d(2026, 11, 24), 30);
        assert!(
            !expired_staged.exists(),
            "expired abandoned staging file must be pruned"
        );
        assert!(!expired_recovery.exists(), "expired recovery archive must be pruned");
        assert!(
            recent_staged.exists(),
            "within-retention staging file must be preserved"
        );
        assert!(
            !abandoned_tmp.exists(),
            "abandoned compression temp file must be cleaned up"
        );
        #[cfg(any(unix, windows))]
        assert!(
            active_tmp.exists(),
            "actively locked compression temp file must be preserved"
        );
        drop(active_handle);
    }
}
