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
                        eprintln!("Failed to rotate daily log file to {}: {err}", daily_log_filename(today));
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
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn_blocking(maintenance);
            } else {
                maintenance();
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
        let Some((date, is_compressed)) = parse_daily_log_name(name_str) else {
            continue;
        };
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
    }
}

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
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // `share_mode(0)` fails with ERROR_SHARING_VIOLATION (32) whenever any
        // writer or redirected child still has the log file open.
        match OpenOptions::new().read(true).write(true).share_mode(0).open(path) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) if err.raw_os_error() == Some(32) => Ok(None),
            Err(err) => Err(err),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        match File::open(path) {
            Ok(file) => Ok(Some(file)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }
}

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
    let tmp_path = gz_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(tmp_name);

    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let tmp_file = options.write(true).create_new(true).open(&tmp_path)?;

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
    #[cfg(windows)]
    drop(input);

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
            eprintln!("Failed to read logs directory '{}' for retention pruning: {err}", dir.display());
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
        let Some((date, is_compressed)) = parse_daily_log_name(name_str) else {
            continue;
        };
        if date >= cutoff {
            continue;
        }
        let path = entry.path();
        if !is_compressed {
            let file = match try_open_exclusive_for_maintenance(&path) {
                Ok(Some(file)) => file,
                Ok(None) => continue,
                Err(err) => {
                    eprintln!("Failed to lock expired log file '{}': {err}", path.display());
                    continue;
                }
            };
            // On Windows, an open handle prevents `remove_file`; on Unix, keep
            // the exclusive lock held across `remove_file` so writers cannot
            // race in between unlock and unlink.
            #[cfg(windows)]
            drop(file);
            if let Err(err) = fs::remove_file(&path)
                && err.kind() != io::ErrorKind::NotFound
            {
                #[cfg(windows)]
                if err.raw_os_error() == Some(32) {
                    continue;
                }
                eprintln!("Failed to remove expired log file '{}': {err}", path.display());
            }
            #[cfg(not(windows))]
            drop(file);
            continue;
        }
        if let Err(err) = fs::remove_file(&path)
            && err.kind() != io::ErrorKind::NotFound
        {
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
        assert_eq!(fs::read_to_string(dir.join("esdiag-2026-11-24.log")).unwrap(), "day one\n");

        // Advance day and write again -> rotates to Nov 25 and gzips Nov 24.
        *clock.lock().unwrap() = d(2026, 11, 25);
        writer.make_writer().write_all(b"day two\n").unwrap();

        assert_eq!(fs::read_to_string(dir.join("esdiag-2026-11-25.log")).unwrap(), "day two\n");
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
        assert!(old.exists(), "huge retention window must preserve files without panicking");
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
}
