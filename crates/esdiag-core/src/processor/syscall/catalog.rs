// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use eyre::{Result, eyre};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Prefix that addresses a syscall source as a processing option.
pub const OPTION_PREFIX: &str = "syscalls.";

/// One entry of the syscall source catalog (`assets/syscalls/sources.yml`),
/// keyed by its `support-diagnostics` `diags.yml` command key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyscallSource {
    /// Bundle-relative output file, the only place a syscall path is defined.
    pub filename: String,
    /// Comma-separated diagnostic types that process this source by default.
    #[serde(default)]
    pub tags: Option<String>,
    /// A syscall processor is registered for this source.
    #[serde(default)]
    pub processable: bool,
    /// ESDiag CPU/time to transform this source (1–5).
    #[serde(default)]
    pub processing_weight: Option<u8>,
    /// Exposed as the user-facing processing option `syscalls.<key>`.
    #[serde(default)]
    pub option: bool,
    /// Applications whose processors implement this source.
    #[serde(default)]
    pub processors: Vec<String>,
}

impl SyscallSource {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags
            .as_deref()
            .is_some_and(|tags| tags.split(',').any(|value| value.trim() == tag))
    }

    pub fn is_processed_by(&self, product: &str) -> bool {
        self.processable && self.processors.iter().any(|processor| processor == product)
    }

    pub fn processing_weight(&self) -> u8 {
        self.processing_weight.unwrap_or(1)
    }
}

static CATALOG: LazyLock<BTreeMap<String, SyscallSource>> =
    LazyLock::new(|| parse_catalog(EMBEDDED_CATALOG).expect("Valid embedded syscall source catalog"));

const EMBEDDED_CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    env!("ESDIAG_ASSETS_DIR"),
    "/syscalls/sources.yml"
));

fn parse_catalog(content: &str) -> Result<BTreeMap<String, SyscallSource>> {
    yaml_serde::from_str(content).map_err(|err| eyre!("Failed to parse syscall source catalog: {err}"))
}

/// The embedded syscall source catalog.
pub fn syscall_sources() -> &'static BTreeMap<String, SyscallSource> {
    &CATALOG
}

/// The catalog entry for `key`.
pub fn syscall_source(key: &str) -> Option<&'static SyscallSource> {
    syscall_sources().get(key)
}

/// The processing option key for a catalog key.
pub fn option_key(key: &str) -> String {
    format!("{OPTION_PREFIX}{key}")
}

pub fn is_option_key(key: &str) -> bool {
    key.starts_with(OPTION_PREFIX)
}

/// Catalog entries that `product` exposes as processing options.
pub fn product_options(product: &str) -> impl Iterator<Item = (&'static String, &'static SyscallSource)> {
    syscall_sources()
        .iter()
        .filter(move |(_, source)| source.option && source.is_processed_by(product))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_resolves_sar() {
        let sar = syscall_source("sar").expect("sar entry");
        assert_eq!(sar.filename, "syscalls/sar.txt");
        assert!(sar.has_tag("standard") && sar.has_tag("support"));
        assert!(!sar.has_tag("minimal") && !sar.has_tag("light"));
        assert!(sar.is_processed_by("elasticsearch"));
        assert!(!sar.is_processed_by("kibana"));
        for input in ["ss", "netstat"] {
            let source = syscall_source(input).expect("correlation input entry");
            assert!(!source.processable && !source.option, "{input} is collect-only");
        }
    }

    #[test]
    fn unknown_catalog_fields_are_rejected() {
        let err = parse_catalog("sar:\n  filename: syscalls/sar.txt\n  tag: standard\n").expect_err("typo");
        assert!(err.to_string().contains("tag"), "{err}");
    }
}
