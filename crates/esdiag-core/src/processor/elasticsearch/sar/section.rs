// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

//! The catalog of `sar -A` sections, recognized by distinctive metric columns
//! rather than by a shared leading `CPU`, `IFACE`, or `INTR` column.

use super::columns::{COUNT, ColumnOverride, FLOAT, KB, KB_RATE, PCT, SECTOR_RATE, SECTORS};
use serde::Serialize;

/// The kind of document a row contributes to, stored in `sar.metricset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Metricset {
    Summary,
    Cpu,
    Io,
    Network,
    Filesystem,
    Interrupt,
    Tty,
    Fan,
    Temp,
    In,
    Usb,
    FcHost,
}

impl Metricset {
    pub fn as_str(self) -> &'static str {
        match self {
            Metricset::Summary => "summary",
            Metricset::Cpu => "cpu",
            Metricset::Io => "io",
            Metricset::Network => "network",
            Metricset::Filesystem => "filesystem",
            Metricset::Interrupt => "interrupt",
            Metricset::Tty => "tty",
            Metricset::Fan => "fan",
            Metricset::Temp => "temp",
            Metricset::In => "in",
            Metricset::Usb => "usb",
            Metricset::FcHost => "fc_host",
        }
    }
}

/// How a section's rows map to documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// No entity column; every row is host-wide.
    Host,
    /// A leading `CPU` column: `all` is host-wide, numbered rows are per CPU.
    PerCpu,
    /// A leading entity column naming the document's `key` field.
    Entity { metricset: Metricset, key: &'static str },
    /// An `INTR` column: `sum` is host-wide, other rows are per interrupt.
    Interrupts,
    /// A leading `CPU` column and one `N/s` column per interrupt.
    InterruptMatrix,
    /// A trailing `FILESYSTEM` and/or `MOUNTPOINT` column.
    Filesystem,
    /// A leading sensor number and a trailing `DEVICE` column.
    Sensor(Metricset),
    /// `BUS idvendor idprod maxpower`, then free-text manufacturer and product.
    Usb,
}

#[derive(Debug)]
pub struct Section {
    pub id: &'static str,
    /// Field prefix, without the trailing dot.
    pub prefix: &'static str,
    /// The first column, when the section has a leading entity column.
    lead: Option<&'static str>,
    /// The header must contain at least one of these columns.
    any_of: &'static [&'static str],
    pub layout: Layout,
    pub overrides: &'static [ColumnOverride],
}

const fn host(id: &'static str, prefix: &'static str, any_of: &'static [&'static str]) -> Section {
    Section {
        id,
        prefix,
        lead: None,
        any_of,
        layout: Layout::Host,
        overrides: &[],
    }
}

const fn per_cpu(
    id: &'static str,
    prefix: &'static str,
    any_of: &'static [&'static str],
    overrides: &'static [ColumnOverride],
) -> Section {
    Section {
        id,
        prefix,
        lead: Some("CPU"),
        any_of,
        layout: Layout::PerCpu,
        overrides,
    }
}

const fn sensor(id: &'static str, lead: &'static str, metricset: Metricset, prefix: &'static str) -> Section {
    Section {
        id,
        prefix,
        lead: Some(lead),
        any_of: &["DEVICE"],
        layout: Layout::Sensor(metricset),
        overrides: &[],
    }
}

const CPU_OVERRIDES: &[ColumnOverride] = &[("%user", "usr.pct", PCT), ("%system", "sys.pct", PCT)];

const DEV_OVERRIDES: &[ColumnOverride] = &[
    ("rkB/s", "read.bytes_per_sec", KB_RATE),
    ("wkB/s", "write.bytes_per_sec", KB_RATE),
    ("dkB/s", "discard.bytes_per_sec", KB_RATE),
    ("rd_sec/s", "read.bytes_per_sec", SECTOR_RATE),
    ("wr_sec/s", "write.bytes_per_sec", SECTOR_RATE),
    ("areq-sz", "areq.bytes", KB),
    ("avgrq-sz", "areq.bytes", SECTORS),
    ("avgqu-sz", "aqu_sz", FLOAT),
];

const IFACE_TRAFFIC_OVERRIDES: &[ColumnOverride] = &[
    ("rxpck/s", "rx.packets_per_sec", FLOAT),
    ("txpck/s", "tx.packets_per_sec", FLOAT),
    ("rxkB/s", "rx.bytes_per_sec", KB_RATE),
    ("txkB/s", "tx.bytes_per_sec", KB_RATE),
    ("rxcmp/s", "rx.compressed_per_sec", FLOAT),
    ("txcmp/s", "tx.compressed_per_sec", FLOAT),
    ("rxmcst/s", "rx.multicast_per_sec", FLOAT),
];

const IFACE_ERROR_OVERRIDES: &[ColumnOverride] = &[
    ("rxerr/s", "rx.errors_per_sec", FLOAT),
    ("txerr/s", "tx.errors_per_sec", FLOAT),
    ("coll/s", "collisions_per_sec", FLOAT),
    ("rxdrop/s", "rx.drops_per_sec", FLOAT),
    ("txdrop/s", "tx.drops_per_sec", FLOAT),
    ("txcarr/s", "tx.carrier_errors_per_sec", FLOAT),
    ("rxfram/s", "rx.frame_errors_per_sec", FLOAT),
    ("rxfifo/s", "rx.fifo_errors_per_sec", FLOAT),
    ("txfifo/s", "tx.fifo_errors_per_sec", FLOAT),
];

/// Every section `sar -A` emits, in recognition order.
pub static SECTIONS: &[Section] = &[
    per_cpu("cpu", "sar.cpu", &["%usr", "%user"], CPU_OVERRIDES),
    per_cpu("softnet", "sar.softnet", &["total/s"], &[]),
    per_cpu("cpu_clock", "sar.cpu", &["MHz"], &[("MHz", "mhz", FLOAT)]),
    per_cpu("cpu_frequency", "sar.cpu", &["wghMHz"], &[("wghMHz", "wghmhz", FLOAT)]),
    host("task", "sar.task", &["proc/s", "cswch/s"]),
    host("swap_paging", "sar.swap", &["pswpin/s"]),
    host("paging", "sar.paging", &["pgpgin/s"]),
    host("block", "sar.block", &["bread/s"]),
    host("memory", "sar.memory", &["kbmemfree"]),
    host("memory_activity", "sar.memory", &["frmpg/s"]),
    host("swap", "sar.swap", &["kbswpfree"]),
    host("hugepages", "sar.hugepages", &["kbhugfree"]),
    host("kernel", "sar.kernel", &["dentunusd"]),
    host("load", "sar.load", &["runq-sz"]),
    host("pressure_cpu", "sar.pressure", &["%scpu-10"]),
    host("pressure_io", "sar.pressure", &["%sio-10"]),
    host("pressure_memory", "sar.pressure", &["%smem-10"]),
    host("sockets", "sar.sockets", &["totsck"]),
    host("sockets6", "sar.sockets", &["tcp6sck"]),
    host("ip", "sar.ip", &["irec/s"]),
    host("eip", "sar.eip", &["ihdrerr/s"]),
    host("icmp", "sar.icmp", &["imsg/s"]),
    host("eicmp", "sar.eicmp", &["ierr/s"]),
    host("tcp", "sar.tcp", &["active/s"]),
    host("etcp", "sar.etcp", &["atmptf/s"]),
    host("udp", "sar.udp", &["idgm/s"]),
    host("ip6", "sar.ip6", &["irec6/s"]),
    host("eip6", "sar.eip6", &["ihdrer6/s"]),
    host("icmp6", "sar.icmp6", &["imsg6/s"]),
    host("eicmp6", "sar.eicmp6", &["ierr6/s"]),
    host("udp6", "sar.udp6", &["idgm6/s"]),
    host("nfs", "sar.nfs", &["call/s"]),
    host("nfsd", "sar.nfsd", &["scall/s"]),
    Section {
        id: "io",
        prefix: "sar.io",
        lead: Some("DEV"),
        any_of: &["tps"],
        layout: Layout::Entity {
            metricset: Metricset::Io,
            key: "sar.io.device",
        },
        overrides: DEV_OVERRIDES,
    },
    Section {
        id: "network",
        prefix: "sar.network",
        lead: Some("IFACE"),
        any_of: &["rxpck/s"],
        layout: Layout::Entity {
            metricset: Metricset::Network,
            key: "sar.network.interface",
        },
        overrides: IFACE_TRAFFIC_OVERRIDES,
    },
    Section {
        id: "network_errors",
        prefix: "sar.network",
        lead: Some("IFACE"),
        any_of: &["rxerr/s"],
        layout: Layout::Entity {
            metricset: Metricset::Network,
            key: "sar.network.interface",
        },
        overrides: IFACE_ERROR_OVERRIDES,
    },
    Section {
        id: "interrupts",
        prefix: "sar.interrupt",
        lead: None,
        any_of: &["intr/s"],
        layout: Layout::Interrupts,
        overrides: &[],
    },
    Section {
        id: "filesystem",
        prefix: "sar.filesystem",
        lead: None,
        any_of: &["FILESYSTEM", "MOUNTPOINT"],
        layout: Layout::Filesystem,
        // `Iused` alone would map `sar.filesystem.iused` as a number and as
        // the parent of `%Iused`'s `iused.pct`.
        overrides: &[("Ifree", "ifree.count", COUNT), ("Iused", "iused.count", COUNT)],
    },
    Section {
        id: "tty",
        prefix: "sar.tty",
        lead: Some("TTY"),
        any_of: &["rcvin/s"],
        layout: Layout::Entity {
            metricset: Metricset::Tty,
            key: "sar.tty.name",
        },
        overrides: &[],
    },
    sensor("fan", "FAN", Metricset::Fan, "sar.fan"),
    sensor("temp", "TEMP", Metricset::Temp, "sar.temp"),
    sensor("in", "IN", Metricset::In, "sar.in"),
    Section {
        id: "usb",
        prefix: "sar.usb",
        lead: Some("BUS"),
        any_of: &["idvendor"],
        layout: Layout::Usb,
        overrides: &[],
    },
    Section {
        id: "fc_host",
        prefix: "sar.fc_host",
        lead: Some("FCHOST"),
        any_of: &["fch_rxf/s"],
        layout: Layout::Entity {
            metricset: Metricset::FcHost,
            key: "sar.fc_host.name",
        },
        overrides: &[],
    },
];

/// The per-interrupt matrix section, recognized by shape rather than by name.
pub static INTERRUPT_MATRIX: Section = Section {
    id: "interrupt_matrix",
    prefix: "sar.interrupt",
    lead: Some("CPU"),
    any_of: &[],
    layout: Layout::InterruptMatrix,
    overrides: &[],
};

/// The interrupt number named by a matrix column (`i007/s` or `7/s`).
pub fn matrix_interrupt(column: &str) -> Option<String> {
    let digits = column.strip_suffix("/s")?;
    let digits = digits.strip_prefix('i').unwrap_or(digits);
    let number: u64 = digits.parse().ok()?;
    Some(number.to_string())
}

/// Identify the section a header's columns (timestamp removed) belong to.
pub fn recognize(columns: &[&str]) -> Option<&'static Section> {
    let first = *columns.first()?;
    if first == "CPU" && columns.len() > 1 && columns[1..].iter().all(|column| matrix_interrupt(column).is_some()) {
        return Some(&INTERRUPT_MATRIX);
    }
    SECTIONS.iter().find(|section| {
        section.lead.is_none_or(|lead| lead == first) && section.any_of.iter().any(|column| columns.contains(column))
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One header per section: observed `sar -A` output where available,
    /// otherwise the sysstat column definitions.
    pub(crate) const FIXTURE_HEADERS: &[(&str, &str)] = &[
        (
            "cpu",
            "CPU %usr %nice %sys %iowait %steal %irq %soft %guest %gnice %idle",
        ),
        ("softnet", "CPU total/s dropd/s squeezd/s rx_rps/s flw_lim/s"),
        ("cpu_clock", "CPU MHz"),
        ("cpu_frequency", "CPU wghMHz"),
        ("task", "proc/s cswch/s"),
        ("swap_paging", "pswpin/s pswpout/s"),
        (
            "paging",
            "pgpgin/s pgpgout/s fault/s majflt/s pgfree/s pgscank/s pgscand/s pgsteal/s %vmeff",
        ),
        ("block", "tps rtps wtps dtps bread/s bwrtn/s bdscd/s"),
        (
            "memory",
            "kbmemfree kbavail kbmemused %memused kbbuffers kbcached kbcommit %commit kbactive kbinact kbdirty kbanonpg kbslab kbkstack kbpgtbl kbvmused",
        ),
        ("memory_activity", "frmpg/s bufpg/s campg/s"),
        ("swap", "kbswpfree kbswpused %swpused kbswpcad %swpcad"),
        ("hugepages", "kbhugfree kbhugused %hugused kbhugrsvd kbhugsurp"),
        ("kernel", "dentunusd file-nr inode-nr pty-nr"),
        ("load", "runq-sz plist-sz ldavg-1 ldavg-5 ldavg-15 blocked"),
        ("pressure_cpu", "%scpu-10 %scpu-60 %scpu-300 %scpu"),
        (
            "pressure_io",
            "%sio-10 %sio-60 %sio-300 %sio %fio-10 %fio-60 %fio-300 %fio",
        ),
        (
            "pressure_memory",
            "%smem-10 %smem-60 %smem-300 %smem %fmem-10 %fmem-60 %fmem-300 %fmem",
        ),
        ("sockets", "totsck tcpsck udpsck rawsck ip-frag tcp-tw"),
        ("sockets6", "tcp6sck udp6sck raw6sck ip6-frag"),
        ("ip", "irec/s fwddgm/s idel/s orq/s asmrq/s asmok/s fragok/s fragcrt/s"),
        (
            "eip",
            "ihdrerr/s iadrerr/s iukwnpr/s idisc/s odisc/s onort/s asmf/s fragf/s",
        ),
        (
            "icmp",
            "imsg/s omsg/s iech/s iechr/s oech/s oechr/s itm/s itmr/s otm/s otmr/s iadrmk/s iadrmkr/s oadrmk/s oadrmkr/s",
        ),
        (
            "eicmp",
            "ierr/s oerr/s idstunr/s odstunr/s itmex/s otmex/s iparmpb/s oparmpb/s isrcq/s osrcq/s iredir/s oredir/s",
        ),
        ("tcp", "active/s passive/s iseg/s oseg/s"),
        ("etcp", "atmptf/s estres/s retrans/s isegerr/s orsts/s"),
        ("udp", "idgm/s odgm/s noport/s idgmerr/s"),
        (
            "ip6",
            "irec6/s fwddgm6/s idel6/s orq6/s asmrq6/s asmok6/s imcpck6/s omcpck6/s fragok6/s fragcr6/s",
        ),
        (
            "eip6",
            "ihdrer6/s iadrer6/s iukwnp6/s i2big6/s idisc6/s odisc6/s inort6/s onort6/s asmf6/s fragf6/s itrpck6/s",
        ),
        (
            "icmp6",
            "imsg6/s omsg6/s iech6/s iechr6/s oechr6/s igmbq6/s igmbr6/s ogmbr6/s igmbrd6/s ogmbrd6/s irtsol6/s ortsol6/s irtad6/s inbsol6/s onbsol6/s inbad6/s onbad6/s",
        ),
        (
            "eicmp6",
            "ierr6/s idtunr6/s odtunr6/s itmex6/s otmex6/s iprmpb6/s oprmpb6/s iredir6/s oredir6/s ipck2b6/s opck2b6/s",
        ),
        ("udp6", "idgm6/s odgm6/s noport6/s idgmer6/s"),
        ("nfs", "call/s retrans/s read/s write/s access/s getatt/s"),
        (
            "nfsd",
            "scall/s badcall/s packet/s udp/s tcp/s hit/s miss/s sread/s swrite/s saccess/s sgetatt/s",
        ),
        ("io", "DEV tps rkB/s wkB/s dkB/s areq-sz aqu-sz await %util"),
        (
            "network",
            "IFACE rxpck/s txpck/s rxkB/s txkB/s rxcmp/s txcmp/s rxmcst/s %ifutil",
        ),
        (
            "network_errors",
            "IFACE rxerr/s txerr/s coll/s rxdrop/s txdrop/s txcarr/s rxfram/s rxfifo/s txfifo/s",
        ),
        ("interrupts", "INTR intr/s"),
        ("interrupt_matrix", "CPU i000/s i001/s i008/s i009/s"),
        (
            "filesystem",
            "MBfsfree MBfsused %fsused %ufsused Ifree Iused %Iused FILESYSTEM",
        ),
        ("tty", "TTY rcvin/s xmtin/s framerr/s prtyerr/s brk/s ovrun/s"),
        ("fan", "FAN rpm drpm DEVICE"),
        ("temp", "TEMP degC %temp DEVICE"),
        ("in", "IN inV %in DEVICE"),
        ("usb", "BUS idvendor idprod maxpower manufact product"),
        ("fc_host", "FCHOST fch_rxf/s fch_txf/s fch_rxw/s fch_txw/s"),
    ];

    fn recognized(header: &str) -> Option<&'static str> {
        let columns: Vec<&str> = header.split_whitespace().collect();
        recognize(&columns).map(|section| section.id)
    }

    #[test]
    fn every_section_has_a_fixture_and_is_recognized() {
        for (id, header) in FIXTURE_HEADERS {
            assert_eq!(recognized(header), Some(*id), "{header}");
        }
        for section in SECTIONS.iter().chain([&INTERRUPT_MATRIX]) {
            assert!(
                FIXTURE_HEADERS.iter().any(|(id, _)| *id == section.id),
                "{} has no fixture header",
                section.id
            );
        }
    }

    #[test]
    fn shared_leading_columns_are_told_apart_by_metrics() {
        assert_eq!(recognized("CPU %user %nice %system %iowait %steal %idle"), Some("cpu"));
        assert_eq!(
            recognized("CPU total/s dropd/s squeezd/s rx_rps/s flw_lim/s blg_len"),
            Some("softnet")
        );
        assert_eq!(recognized("CPU 0/s 1/s 9/s"), Some("interrupt_matrix"));
        assert_eq!(recognized("IFACE rxerr/s txerr/s coll/s"), Some("network_errors"));
        assert_eq!(recognized("IFACE rxpck/s txpck/s"), Some("network"));
        assert_eq!(
            recognized("DEV tps rd_sec/s wr_sec/s avgrq-sz avgqu-sz await svctm %util"),
            Some("io")
        );
        assert_eq!(recognized("tps rtps wtps bread/s bwrtn/s"), Some("block"));
        assert_eq!(recognized("MBfsfree MBfsused %fsused MOUNTPOINT"), Some("filesystem"));
    }

    #[test]
    fn unrecognized_headers_are_unknown() {
        assert_eq!(recognized("CPU"), None);
        assert_eq!(recognized("IFACE"), None);
        assert_eq!(recognized("CPU foo/s bar/s"), None);
        assert_eq!(recognized("widgets/s gadgets/s"), None);
    }
}
