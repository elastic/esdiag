// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Syscall sources: OS command output that a diagnostic bundle stores under
//! `syscalls/`, cataloged separately from REST API sources.

pub mod catalog;
mod runner;

pub use catalog::{SyscallSource, syscall_source, syscall_sources};
pub use runner::{SyscallInput, SyscallLines, SyscallMode, open_syscall};

use eyre::{Result, eyre};
use std::collections::BTreeSet;

/// Check one application's syscall dispatch keys against the catalog: every
/// dispatched key is a processable catalog entry naming `product`, and every
/// such entry is dispatched. Option keys must not collide with REST keys.
pub fn validate_syscall_registry(product: &str, dispatched: &[&str], rest_keys: &[&str]) -> Result<()> {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for key in dispatched {
        if !seen.insert(*key) {
            problems.push(format!("syscall '{key}' is dispatched more than once"));
        }
        match syscall_source(key) {
            None => problems.push(format!("syscall '{key}' is dispatched but not in the syscall catalog")),
            Some(source) if !source.is_processed_by(product) => problems.push(format!(
                "syscall '{key}' is dispatched but its catalog entry is not processable by {product}"
            )),
            Some(_) => {}
        }
    }
    for (key, source) in syscall_sources() {
        if source.is_processed_by(product) && !seen.contains(key.as_str()) {
            problems.push(format!(
                "syscall '{key}' is processable by {product} but has no dispatch entry"
            ));
        }
        if source.option && !source.processable {
            problems.push(format!("syscall '{key}' is an option but not processable"));
        }
        let option = catalog::option_key(key);
        if rest_keys.contains(&option.as_str()) {
            problems.push(format!("syscall option '{option}' collides with a REST source key"));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(eyre!(
            "Invalid {product} syscall dispatch registry: {}",
            problems.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elasticsearch_sar_registry_is_valid() {
        validate_syscall_registry("elasticsearch", &["sar"], &["nodes"]).expect("valid registry");
    }

    #[test]
    fn unclaimed_and_unknown_keys_are_rejected() {
        let err = validate_syscall_registry("elasticsearch", &[], &[]).expect_err("sar unclaimed");
        assert!(err.to_string().contains("'sar' is processable"), "{err}");

        let err = validate_syscall_registry("elasticsearch", &["sar", "ss", "iostat"], &[]).expect_err("bad keys");
        let message = err.to_string();
        assert!(
            message.contains("'ss' is dispatched but its catalog entry"),
            "{message}"
        );
        assert!(message.contains("'iostat' is dispatched but not in"), "{message}");

        let err = validate_syscall_registry("kibana", &["sar"], &[]).expect_err("wrong product");
        assert!(err.to_string().contains("not processable by kibana"), "{err}");
    }

    #[test]
    fn rest_key_collisions_are_rejected() {
        let err = validate_syscall_registry("elasticsearch", &["sar"], &["syscalls.sar"]).expect_err("collision");
        assert!(err.to_string().contains("collides"), "{err}");
    }
}
