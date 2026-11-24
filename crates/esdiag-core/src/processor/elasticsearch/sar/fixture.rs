// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! Synthetic `sar -A` output covering every cataloged section.

use super::section::{Layout, recognize, tests::FIXTURE_HEADERS};

pub const HEADER: &str = "Linux 6.1.0-example (node-01.example.com) \t2026-08-05 \t_x86_64_\t(2 CPU)";
pub const CPUS: usize = 2;
pub const DEVICES: [&str; 3] = ["sda", "sdb", "dm-0"];
pub const INTERFACES: [&str; 2] = ["lo", "eth0"];
pub const MATRIX_INTERRUPTS: usize = 4;

/// Every section, sampled at each of `samples` (`HH:MM:SS AM|PM`), with
/// `Average:` rows closing each section.
pub fn all_sections(samples: &[&str]) -> String {
    let mut text = format!("{HEADER}\n\n");
    for (_, header) in FIXTURE_HEADERS {
        let columns: Vec<&str> = header.split_whitespace().collect();
        let section = recognize(&columns).expect("fixture header is cataloged");
        text.push_str(&format!("12:00:01 AM {header}\n"));
        let mut rows = Vec::new();
        for sample in samples {
            for row in rows_for(section.layout, &columns) {
                rows.push(row.clone());
                text.push_str(&format!("{sample} {row}\n"));
            }
        }
        for row in rows.iter().take(rows.len() / samples.len().max(1)) {
            text.push_str(&format!("Average: {row}\n"));
        }
        text.push('\n');
    }
    text
}

fn values(count: usize) -> String {
    vec!["12"; count].join(" ")
}

fn rows_for(layout: Layout, columns: &[&str]) -> Vec<String> {
    let metrics = |reserved: usize| values(columns.len() - reserved);
    match layout {
        Layout::Host => vec![metrics(0)],
        Layout::PerCpu => std::iter::once("all".to_string())
            .chain((0..CPUS).map(|cpu| cpu.to_string()))
            .map(|cpu| format!("{cpu} {}", metrics(1)))
            .collect(),
        Layout::InterruptMatrix => (0..CPUS).map(|cpu| format!("{cpu} {}", metrics(1))).collect(),
        Layout::Entity { key, .. } => {
            let entities: &[&str] = match key {
                "sar.io.device" => &DEVICES,
                "sar.network.interface" => &INTERFACES,
                "sar.tty.name" => &["0", "1"],
                _ => &["host0"],
            };
            entities
                .iter()
                .map(|entity| format!("{entity} {}", metrics(1)))
                .collect()
        }
        Layout::Interrupts => ["sum", "0", "1"]
            .iter()
            .map(|interrupt| format!("{interrupt} {}", metrics(1)))
            .collect(),
        Layout::Filesystem => vec![format!("{} /dev/sda1", metrics(1))],
        Layout::Sensor(_) => ["1", "2"]
            .iter()
            .map(|id| format!("{id} {} coretemp-isa-0000", metrics(2)))
            .collect(),
        Layout::Usb => vec!["1 1d6b 0002 12 Linux Foundation 2.0 root hub".to_string()],
    }
}
