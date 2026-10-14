//! Browser and clipboard integration for native desktops and WSL.

use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a launcher may run before it counts as having opened the URL.
/// Some launchers, such as `xdg-open`, stay attached to the browser.
const LAUNCHER_EXIT_GRACE: Duration = Duration::from_secs(2);

fn is_wsl() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WSL_INTEROP").is_some() || std::env::var_os("WSL_DISTRO_NAME").is_some())
}

/// Open a URL with the host desktop, including Windows when running in WSL.
pub fn open_browser(url: &str) -> io::Result<()> {
    let mut programs: Vec<(&str, &[&str])> = Vec::new();
    if cfg!(target_os = "windows") || is_wsl() {
        programs.push(("rundll32.exe", &["url.dll,FileProtocolHandler"]));
    }
    if is_wsl() {
        programs.push(("wslview", &[]));
    }
    if cfg!(target_os = "macos") {
        programs.push(("open", &[]));
    } else if cfg!(target_os = "linux") {
        programs.push(("xdg-open", &[]));
    }
    open_with_programs(url, &programs)
}

fn open_with_programs(url: &str, programs: &[(&str, &[&str])]) -> io::Result<()> {
    open_with_programs_within(url, programs, LAUNCHER_EXIT_GRACE)
}

fn open_with_programs_within(url: &str, programs: &[(&str, &[&str])], grace: Duration) -> io::Result<()> {
    let mut error = io::Error::new(io::ErrorKind::NotFound, "No browser launcher available");
    for (program, arguments) in programs {
        let mut child = match Command::new(program)
            .args(*arguments)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                error = err;
                continue;
            }
        };
        let deadline = Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    error = io::Error::other(format!("{program} exited with {status}"));
                    break;
                }
                Ok(None) if Instant::now() >= deadline => {
                    std::thread::spawn(move || child.wait());
                    return Ok(());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(err) => {
                    error = err;
                    break;
                }
            }
        }
    }
    Err(error)
}

/// Copy a value without putting it in process arguments or terminal output.
pub fn copy_to_clipboard(value: &str) -> bool {
    if (cfg!(target_os = "windows") || is_wsl()) && write_clipboard("clip.exe", &[], value, true) {
        return true;
    }
    if cfg!(target_os = "macos") {
        return write_clipboard("pbcopy", &[], value, false);
    }
    if cfg!(target_os = "linux") {
        return [
            ("wl-copy", Vec::new()),
            ("xclip", vec!["-selection", "clipboard"]),
            ("xsel", vec!["--clipboard", "--input"]),
        ]
        .into_iter()
        .any(|(program, args)| write_clipboard(program, &args, value, false));
    }
    false
}

fn write_clipboard(program: &str, arguments: &[&str], value: &str, windows_unicode: bool) -> bool {
    let bytes = if windows_unicode {
        // clip.exe recognizes UTF-16LE by its byte-order mark.
        std::iter::once(0xfeff_u16)
            .chain(value.encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    } else {
        value.as_bytes().to_vec()
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            let result = child.stdin.take().expect("piped clipboard stdin").write_all(&bytes);
            // Dropping stdin delivers EOF before waiting for the clipboard tool.
            if let Err(error) = result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            child.wait()
        })
        .is_ok_and(|status| status.success())
}

#[cfg(all(test, unix))]
mod tests {
    use super::{open_with_programs, open_with_programs_within, write_clipboard};
    use std::time::{Duration, Instant};

    #[test]
    fn browser_falls_back_after_nonzero_exit_and_preserves_url_argument() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("url");
        let output = path.to_str().unwrap();
        let url = "http://localhost:2501/?q=$HOME&name=a;b#fragment";
        open_with_programs(
            url,
            &[
                ("/bin/false", &[]),
                ("/bin/sh", &["-c", "printf '%s' \"$2\" > \"$1\"", "browser", output]),
            ],
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), url);
    }

    #[test]
    fn browser_launcher_that_stays_running_counts_as_opened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fallback");
        let output = path.to_str().unwrap();
        let started = Instant::now();
        open_with_programs_within(
            "http://localhost:2501/",
            &[
                ("/bin/sh", &["-c", "sleep 30"]),
                ("/bin/sh", &["-c", "touch \"$1\"", "fallback", output]),
            ],
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!path.exists(), "a running launcher must not trigger fallbacks");
    }

    #[test]
    fn clipboard_receives_unicode_and_eof_before_exit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clipboard");
        let output = path.to_str().unwrap();
        let value = "pássword 🔑";
        assert!(write_clipboard(
            "/bin/sh",
            &["-c", "cat > \"$1\"", "clipboard", output],
            value,
            true
        ));
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..2], &[0xff, 0xfe]);
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), value);
        assert!(!write_clipboard("/bin/false", &[], "", false));
    }
}
