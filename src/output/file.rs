use serde_json::Value;
use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

pub fn write_ndjson<'a>(value: Value, filename: &PathBuf, append: bool) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .open(filename)?;
    let body = serde_json::to_string(&value).expect("Failed to serialize value");
    file.write_all(body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn write_ndjson_if_debug<'a>(
    value: Value,
    filename: &str,
    append: bool,
) -> std::io::Result<()> {
    let home = match env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".esdiag"),
        Err(_) => panic!("ERROR: No home directory found"),
    };
    if log::log_enabled!(log::Level::Debug) {
        write_ndjson(value, &home.join(filename), append)
    } else {
        Ok(())
    }
}

pub fn append_bulk_docs<'a>(docs: Vec<Value>, filename: &PathBuf) -> std::io::Result<usize> {
    let len = docs.len();
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(filename)?;
    for doc in docs {
        file.write_all(doc.to_string().as_bytes())?;
        file.write_all(b"\n")?;
    }
    Ok(len)
}
