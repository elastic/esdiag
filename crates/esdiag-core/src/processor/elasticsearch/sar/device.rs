// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

/// The driver family of a block device, from its kernel name (`sda`,
/// `dm-0`; `sar -p`) or its `devMAJOR-MINOR` name (plain `sar`).
pub fn device_class(device: &str) -> &'static str {
    match device.strip_prefix("dev").and_then(major_number) {
        Some(major) => class_for_major(major),
        None => class_for_name(device),
    }
}

fn major_number(numbers: &str) -> Option<u32> {
    let (major, minor) = numbers.split_once('-')?;
    minor.parse::<u32>().ok()?;
    major.parse().ok()
}

fn class_for_major(major: u32) -> &'static str {
    match major {
        8 | 65..=71 | 128..=135 => "sd",
        // Dynamically assigned, but device-mapper on common distributions.
        253 => "dm",
        259 => "nvme",
        9 => "md",
        202 => "xvd",
        11 => "sr",
        7 => "loop",
        _ => "other",
    }
}

fn class_for_name(device: &str) -> &'static str {
    let letters_then_digits = |rest: &str| {
        let digits = rest.trim_start_matches(|c: char| c.is_ascii_lowercase());
        digits.len() < rest.len() && digits.bytes().all(|b| b.is_ascii_digit())
    };
    let digits = |rest: &str| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());

    if let Some(rest) = device.strip_prefix("xvd")
        && letters_then_digits(rest)
    {
        "xvd"
    } else if let Some(rest) = device.strip_prefix("sd")
        && letters_then_digits(rest)
    {
        "sd"
    } else if let Some(rest) = device.strip_prefix("vd")
        && letters_then_digits(rest)
    {
        "vd"
    } else if device.strip_prefix("dm-").is_some_and(digits) {
        "dm"
    } else if device.strip_prefix("nvme").is_some_and(is_nvme) {
        "nvme"
    } else if device.strip_prefix("md").is_some_and(digits) {
        "md"
    } else if device.strip_prefix("sr").is_some_and(digits) {
        "sr"
    } else if device.strip_prefix("loop").is_some_and(digits) {
        "loop"
    } else {
        "other"
    }
}

/// `NnN` or `NnNpN` after the `nvme` prefix.
fn is_nvme(rest: &str) -> bool {
    let Some((controller, namespace)) = rest.split_once('n') else {
        return false;
    };
    let namespace = namespace.split_once('p').map_or(namespace, |(namespace, partition)| {
        if partition.is_empty() || !partition.bytes().all(|b| b.is_ascii_digit()) {
            ""
        } else {
            namespace
        }
    });
    [controller, namespace]
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::device_class;

    #[test]
    fn kernel_and_numeric_names_classify_alike() {
        let cases = [
            ("sda", "sd"),
            ("sdb1", "sd"),
            ("sdaa", "sd"),
            ("dm-3", "dm"),
            ("dev8-16", "sd"),
            ("dev65-0", "sd"),
            ("dev253-2", "dm"),
            ("dev11-0", "sr"),
            ("nvme0n1", "nvme"),
            ("nvme0n1p2", "nvme"),
            ("dev259-1", "nvme"),
            ("md127", "md"),
            ("dev9-0", "md"),
            ("vda", "vd"),
            ("vdb2", "vd"),
            ("xvdf", "xvd"),
            ("dev202-0", "xvd"),
            ("sr0", "sr"),
            ("loop4", "loop"),
            ("dev7-4", "loop"),
            ("ram0", "other"),
            ("dev252-0", "other"),
        ];
        for (device, class) in cases {
            assert_eq!(device_class(device), class, "{device}");
        }
    }

    #[test]
    fn lookalike_names_are_other() {
        for device in [
            "sd", "sd1", "dm-", "dm-x", "nvme0", "nvme0n1p", "devices", "dev8", "dev8-x", "sdA", "md",
        ] {
            assert_eq!(device_class(device), "other", "{device}");
        }
    }
}
