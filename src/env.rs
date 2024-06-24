pub const ESDIAG_ES_BULK_SIZE: usize = 5_000;
pub const ESDIAG_ES_WORKERS: usize = 4;
pub static ESDIAG_HOME: &str = ".esdiag";
pub static LOG_LEVEL: &str = "info";

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
