// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use clap::{Parser, ValueEnum};
use eyre::{Result, WrapErr, eyre};
use serde_yaml::{Mapping, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Reconcile ESDiag collection definitions from support-diagnostics (ADR-0006).
///
/// ESDiag owns its per-product `assets/<product>/sources.yml`;
/// `support-diagnostics` is a reconciliation input, not a runtime authority.
/// This tool overlays upstream REST definitions as a field-level merge:
///
/// - upstream owns: `versions`, `extension`, `subdir`, `retry`
/// - ESDiag owns: `tags`, `source_weight`, `processing_weight`, `streamable`,
///   `processable`, `required`, `dependencies`, `collect_dependencies`
///
/// The upstream OS-command catalog (`diags.yml`) is verified for layout drift but
/// deliberately not merged until ESDiag has a command-source transport model.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
struct Args {
    /// Path to a local elastic/support-diagnostics checkout.
    #[arg(long)]
    support_diagnostics: PathBuf,

    /// Product to reconcile. May be repeated; defaults to all products.
    #[arg(long, value_enum)]
    product: Vec<Product>,

    /// Report drift without writing files.
    #[arg(long)]
    check: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
enum Product {
    Elasticsearch,
    Kibana,
    Logstash,
}

impl Product {
    fn key(self) -> &'static str {
        match self {
            Self::Elasticsearch => "elasticsearch",
            Self::Kibana => "kibana",
            Self::Logstash => "logstash",
        }
    }

    fn upstream_file(self) -> &'static str {
        match self {
            Self::Elasticsearch => "src/main/resources/elastic-rest.yml",
            Self::Kibana => "src/main/resources/kibana-rest.yml",
            Self::Logstash => "src/main/resources/logstash-rest.yml",
        }
    }

    fn default_tags(self) -> &'static [&'static str] {
        match self {
            Self::Elasticsearch | Self::Logstash => &["support"],
            Self::Kibana => &["standard", "light", "support"],
        }
    }
}

const UPSTREAM_DIAGS: &str = "src/main/resources/diags.yml";
const UPSTREAM_FIELDS: &[&str] = &["versions", "extension", "subdir", "retry"];
const ESDIAG_FIELDS: &[&str] = &[
    "tags",
    "source_weight",
    "processing_weight",
    "streamable",
    "processable",
    "required",
    "dependencies",
    "collect_dependencies",
];

fn main() -> Result<std::process::ExitCode> {
    let args = Args::parse();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let products = selected_products(&args);
    let mut exit_code = std::process::ExitCode::SUCCESS;

    if !verify_upstream_layout(&args.support_diagnostics, &products) {
        return Ok(std::process::ExitCode::from(2));
    }

    for product in products {
        let upstream_path = args.support_diagnostics.join(product.upstream_file());
        let esdiag_path = repo_root.join("assets").join(product.key()).join("sources.yml");
        let divergences_path = repo_root
            .join("assets")
            .join(product.key())
            .join("sources-divergences.yml");

        let upstream = load_yaml_mapping(&upstream_path)?;
        if upstream.is_empty() {
            println!(
                "[{}] no upstream file at {}, skipping",
                product.key(),
                upstream_path.display()
            );
            continue;
        }

        let esdiag = load_yaml_mapping(&esdiag_path)?;
        let divergences = load_yaml_mapping(&divergences_path)?;
        let (merged, changes) = overlay(product, &esdiag, &upstream, &divergences)?;

        if changes.is_empty() {
            println!("[{}] in sync with upstream", product.key());
            continue;
        }

        println!("[{}] {} change(s):", product.key(), changes.len());
        for change in changes {
            println!("  - {change}");
        }

        if args.check {
            exit_code = std::process::ExitCode::FAILURE;
        } else {
            let content = serde_yaml::to_string(&merged).wrap_err("failed to serialize merged sources")?;
            std::fs::write(&esdiag_path, content)
                .wrap_err_with(|| format!("failed to write {}", esdiag_path.display()))?;
            println!("[{}] wrote {}", product.key(), esdiag_path.display());
            println!(
                "[{}] NOTE: serde_yaml rewrites comments/ordering; review the diff before committing.",
                product.key()
            );
        }
    }

    Ok(exit_code)
}

fn selected_products(args: &Args) -> Vec<Product> {
    if args.product.is_empty() {
        vec![Product::Elasticsearch, Product::Kibana, Product::Logstash]
    } else {
        let mut products = args.product.clone();
        products.sort();
        products.dedup();
        products
    }
}

fn verify_upstream_layout(support_diagnostics: &Path, products: &[Product]) -> bool {
    let mut missing = Vec::new();
    for product in products {
        let path = support_diagnostics.join(product.upstream_file());
        if !path.exists() {
            missing.push(path);
        }
    }

    let diags_path = support_diagnostics.join(UPSTREAM_DIAGS);
    if !diags_path.exists() {
        missing.push(diags_path.clone());
    }

    if !missing.is_empty() {
        eprintln!("[layout] missing expected support-diagnostics file(s):");
        for path in missing {
            eprintln!("  - {}", path.display());
        }
        return false;
    }

    println!(
        "[layout] verified support-diagnostics REST files and OS-command catalog at {}",
        diags_path.display()
    );
    println!("[layout] NOTE: diags.yml is verified but not overlaid until ESDiag has OS-command sources.");
    true
}

fn load_yaml_mapping(path: &Path) -> Result<Mapping> {
    if !path.exists() {
        return Ok(Mapping::new());
    }

    let content = std::fs::read_to_string(path).wrap_err_with(|| format!("failed to read {}", path.display()))?;
    match serde_yaml::from_str::<Value>(&content).wrap_err_with(|| format!("failed to parse {}", path.display()))? {
        Value::Mapping(mapping) => Ok(mapping),
        Value::Null => Ok(Mapping::new()),
        _ => Err(eyre!("{} must contain a YAML mapping", path.display())),
    }
}

fn overlay(
    product: Product,
    esdiag: &Mapping,
    upstream: &Mapping,
    divergences: &Mapping,
) -> Result<(Mapping, Vec<String>)> {
    let mut changes = Vec::new();
    let mut merged = esdiag.clone();

    let renames = string_mapping(divergences, "renames")?;
    let removed = string_set(divergences, "removed")?;
    let owned = string_set(divergences, "esdiag_only")?;

    for (upstream_key, upstream_entry) in upstream {
        let Some(upstream_key) = upstream_key.as_str() else {
            continue;
        };
        if removed.contains(upstream_key) {
            continue;
        }

        let key = renames.get(upstream_key).map(String::as_str).unwrap_or(upstream_key);
        let key_value = Value::String(key.to_string());
        let is_new = !merged.contains_key(&key_value);

        if is_new {
            merged.insert(key_value.clone(), Value::Mapping(Mapping::new()));
        }

        let entry = merged
            .get_mut(&key_value)
            .and_then(Value::as_mapping_mut)
            .ok_or_else(|| eyre!("source entry `{key}` must be a YAML mapping"))?;
        let is_new_source = is_new || !has_any_field(entry, UPSTREAM_FIELDS) && !has_any_field(entry, ESDIAG_FIELDS);
        let upstream_entry = upstream_entry
            .as_mapping()
            .ok_or_else(|| eyre!("upstream source entry `{upstream_key}` must be a YAML mapping"))?;

        for field in UPSTREAM_FIELDS {
            let field_key = Value::String((*field).to_string());
            let Some(value) = upstream_entry.get(&field_key) else {
                if entry.remove(&field_key).is_some() {
                    changes.push(format!("{key}: removed stale upstream-owned `{field}`"));
                }
                continue;
            };

            let value = if *field == "versions" {
                normalize_versions(value)?
            } else {
                value.clone()
            };

            if entry.get(&field_key) != Some(&value) {
                changes.push(format!("{key}: refreshed `{field}` from upstream"));
                entry.insert(field_key, value);
            }
        }

        if ensure_default_tags(entry, product.default_tags()) && !is_new_source {
            changes.push(format!("{key}: added default tag(s) for upstream source"));
        }

        if is_new_source {
            changes.push(format!("{key}: NEW upstream source added (review weights/tags)"));
        }
    }

    let upstream_keys: BTreeSet<String> = upstream
        .keys()
        .filter_map(Value::as_str)
        .map(|key| renames.get(key).cloned().unwrap_or_else(|| key.to_string()))
        .collect();

    for key in merged.keys().filter_map(Value::as_str) {
        if !upstream_keys.contains(key) && !owned.contains(key) {
            changes.push(format!(
                "{key}: not present upstream (esdiag-only; record in divergences if intended)"
            ));
        }
    }

    Ok((merged, changes))
}

fn has_any_field(entry: &Mapping, fields: &[&str]) -> bool {
    fields
        .iter()
        .any(|field| entry.contains_key(Value::String((*field).to_string())))
}

fn ensure_default_tags(entry: &mut Mapping, defaults: &[&str]) -> bool {
    let tags_key = Value::String("tags".to_string());
    let tags = entry.get(&tags_key).and_then(Value::as_str).unwrap_or_default();
    let mut values: Vec<&str> = tags.split(',').map(str::trim).filter(|tag| !tag.is_empty()).collect();
    let mut changed = false;

    for tag in defaults {
        if !values.contains(tag) {
            values.push(tag);
            changed = true;
        }
    }

    if changed {
        entry.insert(tags_key, Value::String(values.join(",")));
    }
    changed
}

fn normalize_versions(value: &Value) -> Result<Value> {
    let mapping = value
        .as_mapping()
        .ok_or_else(|| eyre!("versions must be a YAML mapping"))?;
    let mut normalized = Mapping::new();

    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            return Err(eyre!("version range keys must be strings"));
        };
        normalized.insert(Value::String(normalize_semver_range(key)), value.clone());
    }

    Ok(Value::Mapping(normalized))
}

fn normalize_semver_range(expr: &str) -> String {
    let expr = expr.trim();
    let bytes = expr.as_bytes();
    let mut normalized = String::with_capacity(expr.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let follows_version = normalized.as_bytes().last().is_some_and(|byte| byte.is_ascii_digit());
            let starts_next_clause = index < bytes.len() && matches!(bytes[index], b'<' | b'>' | b'=' | b'~' | b'^');
            if follows_version && starts_next_clause {
                normalized.push_str(", ");
            } else {
                normalized.push_str(&expr[start..index]);
            }
            continue;
        }

        normalized.push(bytes[index] as char);
        index += 1;
    }

    normalized
}

fn string_mapping(mapping: &Mapping, field: &str) -> Result<std::collections::BTreeMap<String, String>> {
    let mut values = std::collections::BTreeMap::new();
    let field_key = Value::String(field.to_string());
    let Some(value) = mapping.get(&field_key) else {
        return Ok(values);
    };
    let value = value
        .as_mapping()
        .ok_or_else(|| eyre!("divergences `{field}` must be a mapping"))?;

    for (key, value) in value {
        let key = key
            .as_str()
            .ok_or_else(|| eyre!("divergences `{field}` keys must be strings"))?;
        let value = value
            .as_str()
            .ok_or_else(|| eyre!("divergences `{field}` values must be strings"))?;
        values.insert(key.to_string(), value.to_string());
    }

    Ok(values)
}

fn string_set(mapping: &Mapping, field: &str) -> Result<BTreeSet<String>> {
    let mut values = BTreeSet::new();
    let field_key = Value::String(field.to_string());
    let Some(value) = mapping.get(&field_key) else {
        return Ok(values);
    };
    let value = value
        .as_sequence()
        .ok_or_else(|| eyre!("divergences `{field}` must be a sequence"))?;

    for value in value {
        let value = value
            .as_str()
            .ok_or_else(|| eyre!("divergences `{field}` values must be strings"))?;
        values.insert(value.to_string());
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::{Mapping, Product, Value, ensure_default_tags, normalize_semver_range, overlay};

    #[test]
    fn ensure_default_tags_adds_missing_tags_without_clobbering_existing_tags() {
        let mut entry = Mapping::new();
        entry.insert(
            Value::String("tags".to_string()),
            Value::String("standard,light".to_string()),
        );

        assert!(ensure_default_tags(&mut entry, &["standard", "light", "support"]));
        assert_eq!(
            entry.get(Value::String("tags".to_string())).and_then(Value::as_str),
            Some("standard,light,support")
        );
        assert!(!ensure_default_tags(&mut entry, &["standard", "light", "support"]));
    }

    #[test]
    fn normalize_semver_range_inserts_clause_commas() {
        assert_eq!(
            normalize_semver_range(">= 5.0.0 < 7.0.0 || >= 8.0.0"),
            ">= 5.0.0, < 7.0.0 || >= 8.0.0"
        );
    }

    #[test]
    fn overlay_removes_stale_upstream_owned_fields() {
        let mut source = Mapping::new();
        source.insert(Value::String("tags".to_string()), Value::String("standard".to_string()));
        source.insert(Value::String("retry".to_string()), Value::Bool(true));
        source.insert(Value::String("subdir".to_string()), Value::String("old".to_string()));
        source.insert(
            Value::String("extension".to_string()),
            Value::String(".txt".to_string()),
        );
        source.insert(
            Value::String("versions".to_string()),
            Value::Mapping(Mapping::from_iter([(
                Value::String(">= 8.0.0".to_string()),
                Value::String("/old".to_string()),
            )])),
        );
        let mut esdiag = Mapping::new();
        esdiag.insert(Value::String("source".to_string()), Value::Mapping(source));

        let mut upstream_source = Mapping::new();
        upstream_source.insert(
            Value::String("versions".to_string()),
            Value::Mapping(Mapping::from_iter([(
                Value::String(">= 8.0.0".to_string()),
                Value::String("/new".to_string()),
            )])),
        );
        let mut upstream = Mapping::new();
        upstream.insert(Value::String("source".to_string()), Value::Mapping(upstream_source));

        let (merged, changes) =
            overlay(Product::Elasticsearch, &esdiag, &upstream, &Mapping::new()).expect("overlay succeeds");
        let source = merged
            .get(Value::String("source".to_string()))
            .and_then(Value::as_mapping)
            .expect("source mapping");

        assert!(!source.contains_key(Value::String("retry".to_string())));
        assert!(!source.contains_key(Value::String("subdir".to_string())));
        assert!(!source.contains_key(Value::String("extension".to_string())));
        assert!(
            changes
                .iter()
                .any(|change| change == "source: removed stale upstream-owned `retry`")
        );
        assert!(
            changes
                .iter()
                .any(|change| change == "source: removed stale upstream-owned `subdir`")
        );
        assert!(
            changes
                .iter()
                .any(|change| change == "source: removed stale upstream-owned `extension`")
        );
    }

    #[test]
    fn overlay_adds_kibana_default_collection_tags() {
        let mut upstream_source = Mapping::new();
        upstream_source.insert(
            Value::String("versions".to_string()),
            Value::Mapping(Mapping::from_iter([(
                Value::String(">= 8.0.0".to_string()),
                Value::String("/api/example".to_string()),
            )])),
        );
        let mut upstream = Mapping::new();
        upstream.insert(Value::String("source".to_string()), Value::Mapping(upstream_source));

        let (merged, _changes) =
            overlay(Product::Kibana, &Mapping::new(), &upstream, &Mapping::new()).expect("overlay succeeds");
        let tags = merged
            .get(Value::String("source".to_string()))
            .and_then(Value::as_mapping)
            .and_then(|entry| entry.get(Value::String("tags".to_string())))
            .and_then(Value::as_str);

        assert_eq!(tags, Some("standard,light,support"));
    }
}
