// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::clean::clean_line;
use super::columns::{COUNT, ColumnSpec, SarValue, ValueKind, column_spec};
use super::device::device_class;
use super::header::SarHeader;
use super::section::{Layout, Metricset, Section, matrix_interrupt, recognize};
use super::timestamp::{RolloverClock, parse_row_time};
use chrono::{NaiveDate, NaiveDateTime};
use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};

static INTR_PER_SEC: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("sar.interrupt.intr_per_sec"));

/// Dotted field paths and their values, in column order.
pub type Fields = Vec<(Arc<str>, SarValue)>;

/// The document a record contributes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The host-wide document for the timestamp.
    Summary,
    /// The per-CPU document for one CPU.
    Cpu(String),
    /// The per-interface document for one interface.
    Network(String),
    /// A document owned by one section, identified by its key fields.
    Entity {
        metricset: Metricset,
        keys: Vec<(&'static str, SarValue)>,
    },
}

/// One row's values for one document.
#[derive(Debug, Clone, PartialEq)]
pub struct SarRecord {
    pub timestamp: NaiveDateTime,
    pub target: Target,
    pub fields: Fields,
}

/// Why a file yielded no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseProblem {
    /// No `Linux …` report header was found.
    NoHeader,
    /// A header was found but no section rows followed.
    NoData,
}

/// What the parser found besides records.
#[derive(Debug, Default)]
pub struct ParseOutcome {
    pub problem: Option<ParseProblem>,
    /// Column lists of distinct unrecognized section headers, in file order.
    pub unknown_headers: Vec<String>,
    /// Report dates read as month/day/year without evidence, in file order.
    pub ambiguous_dates: Vec<NaiveDate>,
}

/// A metric column's full field path and value kind.
#[derive(Debug)]
struct Metric {
    index: usize,
    path: Arc<str>,
    kind: ValueKind,
}

/// A recognized section header compiled into per-column field paths.
#[derive(Debug)]
struct ActiveSection {
    section: &'static Section,
    /// Header column count.
    width: usize,
    metrics: Vec<Metric>,
    /// The `sum` row's paths for the interrupt section.
    summary_metrics: Vec<Metric>,
    /// Column indexes of entity text, by layout.
    entity: Vec<usize>,
    /// Interrupt numbers of a matrix header's columns, by column index.
    interrupts: Vec<Option<String>>,
    /// The interrupt section's `INTR` and `CPU` columns; a header without
    /// `INTR` reports only the host total.
    interrupt_column: Option<usize>,
    cpu_column: Option<usize>,
    filesystem_column: Option<usize>,
    mountpoint_column: Option<usize>,
    /// Where free-text USB manufacturer and product columns start.
    description_column: usize,
}

#[derive(Debug)]
enum State {
    AwaitHeader,
    Between,
    InSection(Box<ActiveSection>),
    Skipping,
}

/// Turns `sar -A` text into records, one line at a time. It never fails:
/// what it cannot use is reported by [`SarParser::finish`].
pub struct SarParser {
    collected: Option<NaiveDate>,
    header: Option<SarHeader>,
    report_date: Option<NaiveDate>,
    state: State,
    clock: Option<RolloverClock>,
    /// The section whose clock is running, kept across a blank line so a
    /// repeated header (sar repeats them, and after `LINUX RESTART`) does not
    /// reset the date.
    last_section: Option<&'static str>,
    unknown: BTreeSet<String>,
    unknown_headers: Vec<String>,
    ambiguous_dates: Vec<NaiveDate>,
    records: usize,
}

impl SarParser {
    pub fn new(collected: Option<NaiveDate>) -> Self {
        Self {
            collected,
            header: None,
            report_date: None,
            state: State::AwaitHeader,
            clock: None,
            last_section: None,
            unknown: BTreeSet::new(),
            unknown_headers: Vec::new(),
            ambiguous_dates: Vec::new(),
            records: 0,
        }
    }

    /// The first report header, once seen.
    pub fn header(&self) -> Option<&SarHeader> {
        self.header.as_ref()
    }

    pub fn push_line(&mut self, raw: &str, out: &mut Vec<SarRecord>) {
        let line = clean_line(raw);
        if line.trim().is_empty() {
            if matches!(self.state, State::InSection(_) | State::Skipping) {
                self.state = State::Between;
            }
            return;
        }
        if let Some(header) = SarHeader::parse(&line, self.collected) {
            if header.ambiguous_date && !self.ambiguous_dates.contains(&header.report_date) {
                self.ambiguous_dates.push(header.report_date);
            }
            self.report_date = Some(header.report_date);
            self.clock = None;
            self.last_section = None;
            self.header.get_or_insert(header);
            self.state = State::Between;
            return;
        }
        let Some(report_date) = self.report_date else {
            return;
        };

        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((time, used)) = parse_row_time(&tokens) else {
            return;
        };
        let columns = &tokens[used..];
        if columns.is_empty() {
            return;
        }
        if columns.len() >= 2 && columns[0] == "LINUX" && columns[1] == "RESTART" {
            if let Some(clock) = self.clock.as_mut() {
                clock.resolve(time);
            }
            return;
        }

        let in_section = matches!(self.state, State::InSection(_));
        if is_header(columns) && !(in_section && is_unavailable_row(columns)) {
            match recognize(columns) {
                Some(section) => {
                    if self.last_section != Some(section.id) || self.clock.is_none() {
                        self.clock = Some(RolloverClock::new(report_date));
                    }
                    self.last_section = Some(section.id);
                    if let Some(clock) = self.clock.as_mut() {
                        clock.resolve(time);
                    }
                    self.state = State::InSection(Box::new(ActiveSection::compile(section, columns)));
                }
                None => {
                    let header = columns.join(" ");
                    if self.unknown.insert(header.clone()) {
                        self.unknown_headers.push(header);
                    }
                    self.last_section = None;
                    self.state = State::Skipping;
                }
            }
            return;
        }

        if let (State::InSection(active), Some(clock)) = (&self.state, self.clock.as_mut()) {
            let timestamp = clock.resolve(time);
            let before = out.len();
            active.emit(timestamp, columns, out);
            self.records += out.len() - before;
        }
    }

    pub fn finish(self) -> ParseOutcome {
        let problem = match (&self.header, self.records) {
            (None, _) => Some(ParseProblem::NoHeader),
            (Some(_), 0) => Some(ParseProblem::NoData),
            _ => None,
        };
        ParseOutcome {
            problem,
            unknown_headers: self.unknown_headers,
            ambiguous_dates: self.ambiguous_dates,
        }
    }
}

/// A header row has no numeric cells.
fn is_header(columns: &[&str]) -> bool {
    columns.iter().all(|column| COUNT.parse(column).is_none())
}

/// A row whose values are all unavailable, `N/A` after an optional entity
/// name, is also free of numeric cells but is not a header.
fn is_unavailable_row(columns: &[&str]) -> bool {
    let unavailable = |column: &&str| column.eq_ignore_ascii_case("N/A");
    columns.iter().any(unavailable) && columns[1..].iter().all(unavailable)
}

impl ActiveSection {
    fn compile(section: &'static Section, columns: &[&str]) -> Self {
        let metric = |index: usize, column: &str, prefix: &str| {
            let ColumnSpec { name, kind } = column_spec(column, section.overrides);
            Metric {
                index,
                path: Arc::from(format!("{prefix}.{name}")),
                kind,
            }
        };
        let metrics_except = |skip: &[usize], prefix: &str| -> Vec<Metric> {
            columns
                .iter()
                .enumerate()
                .filter(|(index, _)| !skip.contains(index))
                .map(|(index, column)| metric(index, column, prefix))
                .collect()
        };
        let position = |name: &str| columns.iter().position(|column| *column == name);

        let mut active = Self {
            section,
            width: columns.len(),
            metrics: Vec::new(),
            summary_metrics: Vec::new(),
            entity: Vec::new(),
            interrupts: Vec::new(),
            interrupt_column: None,
            cpu_column: None,
            filesystem_column: None,
            mountpoint_column: None,
            description_column: columns.len(),
        };
        match section.layout {
            Layout::Host => active.metrics = metrics_except(&[], section.prefix),
            Layout::PerCpu | Layout::Entity { .. } => {
                active.entity = vec![0];
                active.metrics = metrics_except(&[0], section.prefix);
            }
            Layout::Interrupts => {
                let (interrupt, cpu) = (position("INTR"), position("CPU"));
                active.interrupt_column = interrupt;
                active.cpu_column = cpu;
                active.entity = [interrupt, cpu].into_iter().flatten().collect();
                active.metrics = metrics_except(&active.entity, section.prefix);
                active.summary_metrics = metrics_except(&active.entity, "sar.interrupts");
            }
            Layout::InterruptMatrix => {
                active.entity = vec![0];
                active.interrupts = columns.iter().map(|column| matrix_interrupt(column)).collect();
            }
            Layout::Filesystem => {
                let (filesystem, mountpoint) = (position("FILESYSTEM"), position("MOUNTPOINT"));
                active.filesystem_column = filesystem;
                active.mountpoint_column = mountpoint;
                active.entity = [filesystem, mountpoint].into_iter().flatten().collect();
                active.metrics = metrics_except(&active.entity, section.prefix);
            }
            Layout::Sensor(_) => {
                let device = position("DEVICE").unwrap_or(columns.len() - 1);
                active.entity = vec![0, device];
                active.metrics = metrics_except(&active.entity, section.prefix);
            }
            Layout::Usb => {
                let description = position("manufact").unwrap_or(columns.len());
                active.description_column = description;
                active.entity = (0..3).chain(description..columns.len()).collect();
                active.metrics = (3..description)
                    .map(|index| metric(index, columns[index], section.prefix))
                    .collect();
            }
        }
        active
    }

    /// The filesystem column that absorbs a row's extra tokens, such as
    /// spaces in a mount point.
    fn free_text_column(&self) -> Option<usize> {
        self.mountpoint_column.or(self.filesystem_column)
    }

    /// The cell index of a header column, shifted past the free-text column's
    /// extra tokens.
    fn cell_index(&self, column: usize, cells: usize) -> usize {
        match self.free_text_column() {
            Some(free_text) if column > free_text => column + cells.saturating_sub(self.width),
            _ => column,
        }
    }

    /// A text column's value; the free-text column takes its extra tokens.
    fn text_at(&self, cells: &[&str], column: usize) -> Option<String> {
        let start = self.cell_index(column, cells.len());
        let end = if self.free_text_column() == Some(column) {
            start + 1 + cells.len().saturating_sub(self.width)
        } else {
            start + 1
        };
        let text = cells.get(start..end.min(cells.len()))?.join(" ");
        (!text.is_empty()).then_some(text)
    }

    fn fields(&self, metrics: &[Metric], cells: &[&str]) -> Fields {
        metrics
            .iter()
            .filter_map(|metric| {
                let value = metric
                    .kind
                    .parse(cells.get(self.cell_index(metric.index, cells.len()))?)?;
                Some((metric.path.clone(), value))
            })
            .collect()
    }

    fn emit(&self, timestamp: NaiveDateTime, cells: &[&str], out: &mut Vec<SarRecord>) {
        let text = |value: &str| SarValue::Text(value.to_string());
        let mut push = |target: Target, fields: Fields| {
            out.push(SarRecord {
                timestamp,
                target,
                fields,
            })
        };
        match self.section.layout {
            Layout::Host => push(Target::Summary, self.fields(&self.metrics, cells)),
            Layout::PerCpu => {
                let target = match cells[0] {
                    "all" | "ALL" => Target::Summary,
                    cpu => Target::Cpu(cpu.to_string()),
                };
                push(target, self.fields(&self.metrics, cells));
            }
            Layout::Entity {
                metricset: Metricset::Network,
                ..
            } => push(Target::Network(cells[0].to_string()), self.fields(&self.metrics, cells)),
            Layout::Entity { metricset, key } => {
                let mut keys = vec![(key, text(cells[0]))];
                if metricset == Metricset::Io {
                    keys.push(("sar.io.device_class", text(device_class(cells[0]))));
                }
                push(Target::Entity { metricset, keys }, self.fields(&self.metrics, cells));
            }
            Layout::Interrupts => {
                let interrupt = match self.interrupt_column {
                    Some(column) => cells.get(column).copied().unwrap_or_default(),
                    None => "sum",
                };
                let cpu = self.cpu_column.and_then(|column| cells.get(column)).copied();
                let all_cpus = cpu.is_none_or(|cpu| cpu.eq_ignore_ascii_case("all"));
                if interrupt.eq_ignore_ascii_case("sum") && all_cpus {
                    push(Target::Summary, self.fields(&self.summary_metrics, cells));
                } else {
                    let mut keys = vec![("sar.interrupt.id", text(interrupt))];
                    if let Some(cpu) = cpu.filter(|_| !all_cpus) {
                        keys.push(("sar.interrupt.cpu", text(cpu)));
                    }
                    push(
                        Target::Entity {
                            metricset: Metricset::Interrupt,
                            keys,
                        },
                        self.fields(&self.metrics, cells),
                    );
                }
            }
            Layout::InterruptMatrix => {
                let cpu = cells[0];
                let per_cpu = !cpu.eq_ignore_ascii_case("all");
                for (index, interrupt) in self.interrupts.iter().enumerate() {
                    let (Some(interrupt), Some(cell)) = (interrupt, cells.get(index)) else {
                        continue;
                    };
                    let Some(value) = super::columns::FLOAT.parse(cell) else {
                        continue;
                    };
                    let mut keys = vec![("sar.interrupt.id", text(interrupt))];
                    if per_cpu {
                        keys.push(("sar.interrupt.cpu", text(cpu)));
                    }
                    push(
                        Target::Entity {
                            metricset: Metricset::Interrupt,
                            keys,
                        },
                        vec![(INTR_PER_SEC.clone(), value)],
                    );
                }
            }
            Layout::Filesystem => {
                let filesystem = self.filesystem_column.and_then(|column| self.text_at(cells, column));
                let mountpoint = self.mountpoint_column.and_then(|column| self.text_at(cells, column));
                let Some(name) = filesystem.or_else(|| mountpoint.clone()) else {
                    return;
                };
                let mut keys = vec![("sar.filesystem.name", SarValue::Text(name))];
                if let Some(mountpoint) = mountpoint {
                    keys.push(("sar.filesystem.mountpoint", SarValue::Text(mountpoint)));
                }
                push(
                    Target::Entity {
                        metricset: Metricset::Filesystem,
                        keys,
                    },
                    self.fields(&self.metrics, cells),
                );
            }
            Layout::Sensor(metricset) => {
                let id = cells[0];
                let device = cells.get(self.entity[1]).copied().unwrap_or_default();
                let (name_key, device_key, id_key) = match metricset {
                    Metricset::Fan => ("sar.fan.name", "sar.fan.device", "sar.fan.id"),
                    Metricset::Temp => ("sar.temp.name", "sar.temp.device", "sar.temp.id"),
                    _ => ("sar.in.name", "sar.in.device", "sar.in.id"),
                };
                push(
                    Target::Entity {
                        metricset,
                        keys: vec![
                            (name_key, SarValue::Text(format!("{device}/{id}"))),
                            (device_key, text(device)),
                            (id_key, text(id)),
                        ],
                    },
                    self.fields(&self.metrics, cells),
                );
            }
            Layout::Usb => {
                let [bus, vendor, product] = [0, 1, 2].map(|index| cells.get(index).copied().unwrap_or_default());
                let mut keys = vec![
                    ("sar.usb.name", SarValue::Text(format!("{bus}/{vendor}:{product}"))),
                    ("sar.usb.bus", text(bus)),
                    ("sar.usb.idvendor", text(vendor)),
                    ("sar.usb.idprod", text(product)),
                ];
                let description = cells.get(self.description_column..).unwrap_or_default().join(" ");
                if !description.is_empty() {
                    keys.push(("sar.usb.description", SarValue::Text(description)));
                }
                push(
                    Target::Entity {
                        metricset: Metricset::Usb,
                        keys,
                    },
                    self.fields(&self.metrics, cells),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn parse(text: &str) -> (Vec<SarRecord>, ParseOutcome) {
        let mut parser = SarParser::new(NaiveDate::from_ymd_opt(2026, 8, 6));
        let mut records = Vec::new();
        for line in text.lines() {
            parser.push_line(line, &mut records);
        }
        (records, parser.finish())
    }

    fn at(date: &str, time: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn field<'a>(record: &'a SarRecord, path: &str) -> Option<&'a SarValue> {
        record
            .fields
            .iter()
            .find(|(key, _)| key.as_ref() == path)
            .map(|(_, value)| value)
    }

    const HEADER: &str = "Linux 6.1.0 (node-01.example.com) \t08/05/2026 \t_x86_64_\t(2 CPU)";

    #[test]
    fn routes_all_rows_to_summary_and_numbered_rows_to_cpus() {
        let (records, outcome) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM CPU %usr %idle\n12:10:10 AM all 6.10 90.00\n12:10:10 AM 0 5.00 91.00\n12:10:10 AM 1 7.20 89.00\nAverage: all 6.10 90.00\n"
        ));
        assert_eq!(outcome.problem, None);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].target, Target::Summary);
        assert_eq!(records[0].timestamp, at("2026-08-05", "00:10:10"));
        assert_eq!(field(&records[0], "sar.cpu.usr.pct"), Some(&SarValue::Float(0.061)));
        assert_eq!(records[1].target, Target::Cpu("0".to_string()));
        assert_eq!(records[2].target, Target::Cpu("1".to_string()));
    }

    #[test]
    fn older_and_newer_sysstat_columns_share_field_names() {
        let (old, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM DEV tps rd_sec/s wr_sec/s avgrq-sz avgqu-sz await svctm %util\n12:10:01 AM dev8-0 1.00 8.00 16.00 16.00 0.10 2.00 1.00 0.50\n\n12:00:01 AM CPU %user %system %idle\n12:10:01 AM all 6.10 1.00 92.90\n"
        ));
        assert_eq!(
            field(&old[0], "sar.io.read.bytes_per_sec"),
            Some(&SarValue::Float(4096.0))
        );
        assert_eq!(
            field(&old[0], "sar.io.write.bytes_per_sec"),
            Some(&SarValue::Float(8192.0))
        );
        assert_eq!(field(&old[0], "sar.io.areq.bytes"), Some(&SarValue::Integer(8192)));
        assert_eq!(field(&old[0], "sar.io.aqu_sz"), Some(&SarValue::Float(0.1)));
        assert!(field(&old[0], "sar.io.svctm_ms").is_some());
        assert_eq!(field(&old[1], "sar.cpu.usr.pct"), Some(&SarValue::Float(0.061)));
        assert!(field(&old[1], "sar.cpu.sys.pct").is_some());

        let (new, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM DEV tps rkB/s wkB/s dkB/s areq-sz aqu-sz await zq-cnt/s %util\n12:10:01 AM sda 1.00 4.00 8.00 0.00 7.68 0.10 2.00 3.50 0.50\n"
        ));
        assert_eq!(
            new[0].target,
            Target::Entity {
                metricset: Metricset::Io,
                keys: vec![
                    ("sar.io.device", SarValue::Text("sda".into())),
                    ("sar.io.device_class", SarValue::Text("sd".into())),
                ]
            }
        );
        assert_eq!(
            field(&new[0], "sar.io.read.bytes_per_sec"),
            Some(&SarValue::Float(4096.0))
        );
        assert_eq!(field(&new[0], "sar.io.areq.bytes"), Some(&SarValue::Integer(7864)));
        assert_eq!(field(&new[0], "sar.io.zq_cnt_per_sec"), Some(&SarValue::Float(3.5)));
        assert!(
            field(&new[0], "sar.io.svctm_ms").is_none(),
            "absent columns stay absent"
        );
    }

    #[test]
    fn each_section_restarts_the_clock_at_the_report_date() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM CPU %usr\n12:10:00 AM all 1.00\n11:50:00 PM all 1.00\n12:00:01 AM all 1.00\n\n12:00:01 AM kbmemfree\n12:10:00 AM 100\n"
        ));
        let times: Vec<NaiveDateTime> = records.iter().map(|record| record.timestamp).collect();
        assert_eq!(
            times,
            [
                at("2026-08-05", "00:10:00"),
                at("2026-08-05", "23:50:00"),
                at("2026-08-06", "00:00:01"),
                at("2026-08-05", "00:10:00"),
            ]
        );
    }

    #[test]
    fn repeated_headers_and_restarts_keep_the_section_clock() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n11:40:00 PM CPU %usr\n11:50:00 PM all 1.00\n\n12:05:00 AM LINUX RESTART (2 CPU)\n\n12:05:00 AM CPU %usr\n12:10:00 AM all 2.00\n\n01:00:00 AM CPU %usr\n01:10:00 AM all 3.00\n"
        ));
        let times: Vec<NaiveDateTime> = records.iter().map(|record| record.timestamp).collect();
        assert_eq!(
            times,
            [
                at("2026-08-05", "23:50:00"),
                at("2026-08-06", "00:10:00"),
                at("2026-08-06", "01:10:00"),
            ]
        );
    }

    #[test]
    fn unknown_sections_are_skipped_and_reported_once() {
        let (records, outcome) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM proc/s cswch/s\n12:10:00 AM 1.00 2.00\n\n12:00:01 AM widgets/s gadgets/s\n12:10:00 AM 9.00 9.00\n\n12:20:00 AM widgets/s gadgets/s\n12:30:00 AM 9.00 9.00\n\n12:00:01 AM runq-sz ldavg-1\n12:10:00 AM 3 0.50\n"
        ));
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.fields.iter().all(|(_, value)| *value != SarValue::Float(9.0)))
        );
        assert_eq!(field(&records[1], "sar.load.runq_sz"), Some(&SarValue::Integer(3)));
        assert_eq!(outcome.unknown_headers, ["widgets/s gadgets/s"]);
        assert_eq!(outcome.problem, None);
    }

    #[test]
    fn an_unknown_header_right_after_known_rows_starts_a_skipped_section() {
        let (records, outcome) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM proc/s cswch/s\n12:10:00 AM 1.00 2.00\n12:00:01 AM widgets/s gadgets/s\n12:10:00 AM 9.00 9.00\n"
        ));
        assert_eq!(records.len(), 1);
        assert_eq!(field(&records[0], "sar.task.proc_per_sec"), Some(&SarValue::Float(1.0)));
        assert_eq!(outcome.unknown_headers, ["widgets/s gadgets/s"]);
    }

    #[test]
    fn unavailable_rows_inside_a_section_are_rows_not_headers() {
        let (records, outcome) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM IFACE rxpck/s txpck/s\n12:10:00 AM eth0 N/A N/A\n12:10:00 AM eth1 4.00 5.00\n"
        ));
        assert!(outcome.unknown_headers.is_empty());
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].target, Target::Network("eth1".to_string()));
        assert_eq!(
            field(&records[1], "sar.network.rx.packets_per_sec"),
            Some(&SarValue::Float(4.0))
        );
    }

    #[test]
    fn filesystem_names_come_from_their_header_column() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM FILESYSTEM MBfsfree %fsused MOUNTPOINT\n12:10:00 AM /dev/sda1 2 33.33 /mnt/data volume\n"
        ));
        let Target::Entity { keys, .. } = &records[0].target else {
            panic!("filesystem document");
        };
        assert_eq!(
            keys,
            &[
                ("sar.filesystem.name", SarValue::Text("/dev/sda1".into())),
                ("sar.filesystem.mountpoint", SarValue::Text("/mnt/data volume".into())),
            ]
        );
        assert_eq!(
            field(&records[0], "sar.filesystem.fsfree.bytes"),
            Some(&SarValue::Integer(2_097_152))
        );
        assert_eq!(
            field(&records[0], "sar.filesystem.fsused.pct"),
            Some(&SarValue::Float(0.3333))
        );
    }

    #[test]
    fn ambiguous_report_dates_are_reported_once() {
        let mut parser = SarParser::new(None);
        let mut records = Vec::new();
        for line in [
            "Linux 6.1.0 (node-01.example.com) \t04/08/2026",
            "",
            "12:00:01 AM proc/s",
            "12:10:00 AM 1.00",
            "Linux 6.1.0 (node-01.example.com) \t04/08/2026",
        ] {
            parser.push_line(line, &mut records);
        }
        assert_eq!(
            parser.finish().ambiguous_dates,
            [NaiveDate::from_ymd_opt(2026, 4, 8).unwrap()]
        );
    }

    #[test]
    fn ansi_decorated_input_parses_like_clean_input() {
        let clean = format!("{HEADER}\n\n12:00:01 AM CPU %usr %idle\n12:10:10 AM all 6.10 90.00\n");
        let decorated = format!(
            "\x1b[1m{HEADER}\x1b[0m\r\n\r\n\x1b[1;32m12:00:01 AM\x1b[0m CPU %usr %idle\r\n12:10:10 AM\x00 \x1b[31mall\x1b[0m 6.10 90.00\r\n"
        );
        assert_eq!(parse(&clean).0, parse(&decorated).0);
    }

    #[test]
    fn interrupt_matrix_is_transposed_per_cpu() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM CPU i000/s i009/s\n12:10:00 AM 0 1.00 2.00\n12:10:00 AM 1 3.00 4.00\n"
        ));
        assert_eq!(records.len(), 4);
        assert_eq!(
            records[1],
            SarRecord {
                timestamp: at("2026-08-05", "00:10:00"),
                target: Target::Entity {
                    metricset: Metricset::Interrupt,
                    keys: vec![
                        ("sar.interrupt.id", SarValue::Text("9".into())),
                        ("sar.interrupt.cpu", SarValue::Text("0".into())),
                    ],
                },
                fields: vec![(Arc::from("sar.interrupt.intr_per_sec"), SarValue::Float(2.0))],
            }
        );
    }

    #[test]
    fn interrupt_sum_goes_to_summary() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM INTR intr/s\n12:10:00 AM sum 500.00\n12:10:00 AM 0 12.00\n"
        ));
        assert_eq!(records[0].target, Target::Summary);
        assert_eq!(
            field(&records[0], "sar.interrupts.intr_per_sec"),
            Some(&SarValue::Float(500.0))
        );
        assert!(
            matches!(&records[1].target, Target::Entity { metricset: Metricset::Interrupt, keys } if keys.len() == 1)
        );
        assert_eq!(
            field(&records[1], "sar.interrupt.intr_per_sec"),
            Some(&SarValue::Float(12.0))
        );
    }

    #[test]
    fn interrupt_headers_without_an_intr_column_report_the_total() {
        let (records, _) = parse(&format!("{HEADER}\n\n12:00:01 AM intr/s\n12:10:00 AM 500.00\n"));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].target, Target::Summary);
        assert_eq!(
            field(&records[0], "sar.interrupts.intr_per_sec"),
            Some(&SarValue::Float(500.0))
        );

        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM CPU intr/s\n12:10:00 AM all 500.00\n12:10:00 AM 0 12.00\n"
        ));
        assert_eq!(records[0].target, Target::Summary);
        assert!(matches!(
            &records[1].target,
            Target::Entity { metricset: Metricset::Interrupt, keys }
                if keys.contains(&("sar.interrupt.cpu", SarValue::Text("0".to_string())))
        ));
    }

    #[test]
    fn usb_descriptions_keep_their_spaces() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM BUS idvendor idprod maxpower manufact product\n12:10:00 AM 1 1d6b 0002 0 Linux Foundation 2.0 root hub\n"
        ));
        let Target::Entity {
            metricset: Metricset::Usb,
            keys,
        } = &records[0].target
        else {
            panic!("usb document");
        };
        assert!(keys.contains(&("sar.usb.name", SarValue::Text("1/1d6b:0002".into()))));
        assert!(keys.contains(&(
            "sar.usb.description",
            SarValue::Text("Linux Foundation 2.0 root hub".into())
        )));
        assert_eq!(field(&records[0], "sar.usb.maxpower"), Some(&SarValue::Integer(0)));
    }

    #[test]
    fn filesystems_and_sensors_are_keyed_by_name() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM MBfsfree MBfsused %fsused %ufsused Ifree Iused %Iused FILESYSTEM\n12:10:00 AM 2 1 33.33 33.33 100 50 33.33 /dev/mapper/root\n\n12:00:01 AM TEMP degC %temp DEVICE\n12:10:00 AM 1 45.00 50.00 coretemp-isa-0000\n"
        ));
        let Target::Entity {
            metricset: Metricset::Filesystem,
            keys,
        } = &records[0].target
        else {
            panic!("filesystem document");
        };
        assert_eq!(
            keys,
            &[("sar.filesystem.name", SarValue::Text("/dev/mapper/root".into()))]
        );
        assert_eq!(
            field(&records[0], "sar.filesystem.fsfree.bytes"),
            Some(&SarValue::Integer(2_097_152))
        );
        let Target::Entity {
            metricset: Metricset::Temp,
            keys,
        } = &records[1].target
        else {
            panic!("temp document");
        };
        assert!(keys.contains(&("sar.temp.name", SarValue::Text("coretemp-isa-0000/1".into()))));
        assert_eq!(field(&records[1], "sar.temp.degc"), Some(&SarValue::Float(45.0)));
    }

    #[test]
    fn mountpoint_only_filesystems_use_it_as_the_name() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM MBfsfree MBfsused %fsused MOUNTPOINT\n12:10:00 AM 2 1 33.33 /mnt/data volume\n"
        ));
        let Target::Entity { keys, .. } = &records[0].target else {
            panic!("filesystem document");
        };
        assert_eq!(
            keys,
            &[
                ("sar.filesystem.name", SarValue::Text("/mnt/data volume".into())),
                ("sar.filesystem.mountpoint", SarValue::Text("/mnt/data volume".into())),
            ]
        );
    }

    #[test]
    fn missing_headers_and_empty_reports_are_problems() {
        assert_eq!(
            parse("bash: sar: command not found\n").1.problem,
            Some(ParseProblem::NoHeader)
        );
        assert_eq!(parse(&format!("{HEADER}\n\n")).1.problem, Some(ParseProblem::NoData));
        assert_eq!(
            parse(&format!("{HEADER}\n\n12:00:01 AM CPU %usr\nAverage: all 1.00\n"))
                .1
                .problem,
            Some(ParseProblem::NoData)
        );
    }

    #[test]
    fn non_numeric_cells_are_omitted() {
        let (records, _) = parse(&format!(
            "{HEADER}\n\n12:00:01 AM CPU MHz\n12:10:00 AM all N/A\n12:10:00 AM 0 2400.00\n"
        ));
        assert!(records[0].fields.is_empty());
        assert_eq!(field(&records[1], "sar.cpu.mhz"), Some(&SarValue::Float(2400.0)));
    }
}
