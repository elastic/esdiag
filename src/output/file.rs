use serde_json::Value;
use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

pub fn write_ndjson<'a>(input: &str, value: Value, filename: &PathBuf) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(filename)?;
    let body = serde_json::to_string(&value).unwrap();
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    log::info!("{}: appended {input}", filename.display());
    Ok(())
}

pub fn write_ndjson_if_debug<'a>(input: &str, value: Value, filename: &str) -> std::io::Result<()> {
    let home = match env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".esdiag"),
        Err(_) => panic!("ERROR: No home directory found"),
    };
    if log::log_enabled!(log::Level::Debug) {
        write_ndjson(input, value, &home.join(filename))
    } else {
        Ok(())
    }
}

pub fn write_bulk_docs<'a>(docs: Vec<Value>, filename: &PathBuf) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(filename)?;
    let len = docs.len();
    for doc in docs {
        file.write_all(doc.to_string().as_bytes())?;
        file.write_all(b"\n")?;
    }
    log::info!("{}: appended {} docs", filename.display(), len);
    Ok(())
}
