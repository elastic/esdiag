// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Kubernetes resource quantities (`500m`, `16380820Ki`, `1e3`) as numbers:
//! CPU in cores, memory and storage in bytes.

use serde::{Serialize, Serializer};
use std::collections::BTreeMap;

const BINARY_SUFFIXES: [(&str, i32); 6] = [("Ki", 1), ("Mi", 2), ("Gi", 3), ("Ti", 4), ("Pi", 5), ("Ei", 6)];
const DECIMAL_SUFFIXES: [(char, i32); 9] = [
    ('n', -9),
    ('u', -6),
    ('m', -3),
    ('k', 3),
    ('M', 6),
    ('G', 9),
    ('T', 12),
    ('P', 15),
    ('E', 18),
];

/// Resource names whose quantities are indexed; anything else (hugepages,
/// extended device resources) is dropped to keep the mapping bounded.
const INDEXED_RESOURCES: [&str; 5] = ["cpu", "memory", "ephemeral-storage", "pods", "storage"];

pub fn parse(quantity: &str) -> Option<f64> {
    let quantity = quantity.trim();
    let (number, base, power) = if let Some((number, power)) = BINARY_SUFFIXES
        .iter()
        .find_map(|(suffix, power)| quantity.strip_suffix(suffix).map(|number| (number, *power)))
    {
        (number, 1024f64, power)
    } else if let Some((number, power)) = quantity.chars().last().and_then(|last| {
        DECIMAL_SUFFIXES
            .iter()
            .find(|(suffix, _)| *suffix == last)
            .map(|(_, power)| (&quantity[..quantity.len() - last.len_utf8()], *power))
    }) {
        (number, 10f64, power)
    } else {
        (quantity, 10f64, 0)
    };

    if number.is_empty()
        || !number
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E'))
    {
        return None;
    }
    let number = number.parse::<f64>().ok()?;
    // Dividing keeps milli values exact in decimal (`1900m` is 1.9, not 1.9000000000000001).
    let value = if power < 0 {
        number / base.powi(-power)
    } else {
        number * base.powi(power)
    };
    value.is_finite().then_some(value)
}

/// Parsed quantities keyed by their Kubernetes resource name, omitting any
/// value that does not parse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quantities(pub BTreeMap<String, f64>);

impl Quantities {
    pub fn from_raw(raw: Option<&BTreeMap<String, String>>) -> Self {
        let parsed = raw
            .into_iter()
            .flatten()
            .filter(|(name, _)| INDEXED_RESOURCES.contains(&name.as_str()))
            .filter_map(|(name, value)| parse(value).map(|value| (name.clone(), value)))
            .collect();
        Self(parsed)
    }

    #[cfg(test)]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.0.get(name).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Quantities {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_representative_quantities() {
        assert_eq!(parse("500m"), Some(0.5));
        assert_eq!(parse("4"), Some(4.0));
        assert_eq!(parse("16380820Ki"), Some(16_773_959_680.0));
        assert_eq!(parse("10Gi"), Some(10_737_418_240.0));
        assert_eq!(parse("1e3"), Some(1000.0));
        assert_eq!(parse("18230014742"), Some(18_230_014_742.0));
    }

    #[test]
    fn parses_decimal_suffixes_and_fractions() {
        assert_eq!(parse("1k"), Some(1000.0));
        assert_eq!(parse("2M"), Some(2_000_000.0));
        assert_eq!(parse("1.5G"), Some(1_500_000_000.0));
        assert_eq!(parse("1E"), Some(1e18));
        assert_eq!(parse("0.5Mi"), Some(524_288.0));
        assert_eq!(parse("1900m"), Some(1.9));
    }

    #[test]
    fn rejects_invalid_quantities() {
        for invalid in ["", "not-a-quantity", "Gi", "inf", "NaN", "1Xi", "12 Gi"] {
            assert_eq!(parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn quantities_drop_unparseable_and_unindexed_resources() {
        let raw = BTreeMap::from([
            ("cpu".to_string(), "not-a-quantity".to_string()),
            ("memory".to_string(), "4Gi".to_string()),
            ("hugepages-2Mi".to_string(), "0".to_string()),
        ]);
        let quantities = Quantities::from_raw(Some(&raw));
        assert_eq!(quantities.get("cpu"), None);
        assert_eq!(quantities.get("memory"), Some(4_294_967_296.0));
        assert_eq!(quantities.0.len(), 1);
    }
}
