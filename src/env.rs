// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

pub const ESDIAG_ES_BULK_SIZE: usize = 5_000;
pub const ESDIAG_ES_WORKERS: usize = 4;
pub static ESDIAG_HOME: &str = ".esdiag";
pub static LOG_LEVEL: &str = "info";
pub static ESDIAG_KIBANA_URL: &str = "http://localhost:5601";
pub static ESDIAG_KIBANA_DEFAULT_SPACE: &str = "esdiag";
pub static ESDIAG_KEYSTORE_PASSWORD: &str = "ESDIAG_KEYSTORE_PASSWORD";

fn default_int(name: &str) -> Option<usize> {
    match name {
        "ESDIAG_ES_BULK_SIZE" => Some(ESDIAG_ES_BULK_SIZE),
        "ESDIAG_ES_WORKERS" => Some(ESDIAG_ES_WORKERS),
        _ => None,
    }
}

fn default_str(name: &str) -> Option<&str> {
    match name {
        "ESDIAG_HOME" => Some(ESDIAG_HOME),
        "LOG_LEVEL" => Some(LOG_LEVEL),
        "ESDIAG_KIBANA_URL" => Some(ESDIAG_KIBANA_URL),
        _ => None,
    }
}

pub fn get_int(name: &str) -> std::io::Result<usize> {
    let env = std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let default = default_int(name);

    env.or(default).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} not found", name))
    })
}

pub fn get_string(name: &str) -> std::io::Result<String> {
    let env = std::env::var(name).ok();
    let default = default_str(name);

    env.or(default.map(|s| s.to_string())).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} not found", name))
    })
}

pub fn get_kibana_space() -> Option<String> {
    match std::env::var("ESDIAG_KIBANA_SPACE") {
        Ok(space) => {
            let trimmed = space.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => Some(ESDIAG_KIBANA_DEFAULT_SPACE.to_string()),
    }
}

pub fn append_kibana_space(kibana_url: &str) -> String {
    let kibana_url = kibana_url.trim_end_matches('/');
    match get_kibana_space() {
        Some(space) => {
            if let Ok(mut url) = url::Url::parse(kibana_url) {
                if let Some(existing_segments) = url.path_segments() {
                    let mut segments = existing_segments
                        .filter(|segment| !segment.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    if segments.first().map(String::as_str) == Some("s") {
                        if let Some(current_space) = segments.get_mut(1) {
                            *current_space = space;
                        } else {
                            segments.push(space);
                        }
                    } else {
                        segments.insert(0, space);
                        segments.insert(0, "s".to_string());
                    }
                    url.set_path(&format!("/{}", segments.join("/")));
                    return url.to_string();
                }
            }
            format!("{kibana_url}/s/{space}")
        }
        None => kibana_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{append_kibana_space, get_kibana_space};
    use std::sync::Mutex;

    fn env_lock() -> &'static Mutex<()> {
        crate::test_env_lock()
    }

    #[test]
    fn default_kibana_space_is_esdiag() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::remove_var("ESDIAG_KIBANA_SPACE");
        }

        assert_eq!(get_kibana_space().as_deref(), Some("esdiag"));
    }

    #[test]
    fn append_kibana_space_replaces_existing_space_and_preserves_path() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_SPACE", "support");
        }

        assert_eq!(
            append_kibana_space("https://kb:5601/s/foo/app/home"),
            "https://kb:5601/s/support/app/home"
        );
    }

    #[test]
    fn append_kibana_space_inserts_space_before_existing_path() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_SPACE", "support");
        }

        assert_eq!(
            append_kibana_space("https://kb:5601/app/home?foo=bar#hash"),
            "https://kb:5601/s/support/app/home?foo=bar#hash"
        );
    }

    #[test]
    fn append_kibana_space_omits_space_segment_when_env_is_empty() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            std::env::set_var("ESDIAG_KIBANA_SPACE", "");
        }

        assert_eq!(
            append_kibana_space("https://kb:5601/app/home"),
            "https://kb:5601/app/home"
        );
    }
}
