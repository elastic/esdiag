// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use chrono::{Datelike, NaiveDate};

/// The `Linux <kernel> (<host>) <date> [_<arch>_] [(<n> CPU)]` line that opens
/// every `sar` report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SarHeader {
    pub hostname: String,
    pub kernel: String,
    pub architecture: Option<String>,
    pub cpu_count: Option<u32>,
    pub report_date: NaiveDate,
    /// Day and month could be either order and the collection date did not
    /// decide, so month/day/year was assumed.
    pub ambiguous_date: bool,
}

impl SarHeader {
    /// Parse a header line. `collected` disambiguates day/month order and
    /// two-digit years.
    pub fn parse(line: &str, collected: Option<NaiveDate>) -> Option<Self> {
        let rest = line.trim_start().strip_prefix("Linux")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        let (kernel, rest) = rest.trim_start().split_once(char::is_whitespace)?;
        let rest = rest.trim_start().strip_prefix('(')?;
        let (hostname, rest) = rest.split_once(')')?;
        let mut tokens = rest.split_whitespace().peekable();
        let (report_date, ambiguous_date) = parse_report_date(tokens.next()?, collected)?;

        let mut architecture = None;
        let mut cpu_count = None;
        while let Some(token) = tokens.next() {
            if let Some(arch) = token.strip_prefix('_').and_then(|arch| arch.strip_suffix('_')) {
                architecture = Some(arch.to_string());
            } else if let Some(count) = token.strip_prefix('(')
                && tokens.peek().is_some_and(|next| next.starts_with("CPU"))
            {
                cpu_count = count.parse().ok();
            }
        }

        Some(Self {
            hostname: hostname.trim().to_string(),
            kernel: kernel.to_string(),
            architecture,
            cpu_count,
            report_date,
            ambiguous_date,
        })
    }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Resolve a header date in any locale `sar` prints: `YYYY-MM-DD`,
/// `DD-Mon-YYYY`, or `A/B/Y` with `/`, `.`, or `-` separators where either
/// `A` or `B` is the day. The flag is set when month/day/year was assumed.
fn parse_report_date(token: &str, collected: Option<NaiveDate>) -> Option<(NaiveDate, bool)> {
    let mut parts = token.split(['/', '.', '-']);
    let (Some(first), Some(second), Some(third), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if first.len() == 4 {
        let date = NaiveDate::from_ymd_opt(first.parse().ok()?, second.parse().ok()?, third.parse().ok()?)?;
        return Some((date, false));
    }
    let year = expand_year(third, collected)?;
    if let Some(month) = MONTHS
        .iter()
        .position(|month| second.get(..3).is_some_and(|name| name.eq_ignore_ascii_case(month)))
    {
        return Some((
            NaiveDate::from_ymd_opt(year, month as u32 + 1, first.parse().ok()?)?,
            false,
        ));
    }

    let a: u32 = first.parse().ok()?;
    let b: u32 = second.parse().ok()?;
    let month_first = NaiveDate::from_ymd_opt(year, a, b);
    let day_first = NaiveDate::from_ymd_opt(year, b, a);
    match (month_first, day_first) {
        (Some(date), None) | (None, Some(date)) => Some((date, false)),
        (None, None) => None,
        (Some(mdy), Some(dmy)) if mdy == dmy => Some((mdy, false)),
        (Some(mdy), Some(dmy)) => {
            let on_or_before = |date: NaiveDate| collected.is_some_and(|collected| date <= collected);
            match (on_or_before(mdy), on_or_before(dmy)) {
                (true, true) => Some((mdy.max(dmy), false)),
                (true, false) => Some((mdy, false)),
                (false, true) => Some((dmy, false)),
                (false, false) => Some((mdy, true)),
            }
        }
    }
}

/// Expand a two-digit year to the century that does not place the report
/// after the collection year.
fn expand_year(token: &str, collected: Option<NaiveDate>) -> Option<i32> {
    let year: i32 = token.parse().ok()?;
    match token.len() {
        4 => Some(year),
        2 => {
            let collected_year = collected.map(|date| date.year()).unwrap_or(2000 + year);
            let century = collected_year - collected_year.rem_euclid(100);
            let candidate = century + year;
            Some(if candidate > collected_year {
                candidate - 100
            } else {
                candidate
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn report_date(token: &str, collected: Option<NaiveDate>) -> NaiveDate {
        SarHeader::parse(
            &format!("Linux 6.1.0 (node-01) \t{token} \t_x86_64_\t(4 CPU)"),
            collected,
        )
        .expect("header")
        .report_date
    }

    #[test]
    fn parses_fqdn_architecture_and_cpu_count() {
        let header = SarHeader::parse(
            "Linux 4.18.0-553.97.1.el8_10.x86_64 (node-01.example.com) \t02/21/2026 \t_x86_64_\t(32 CPU)",
            None,
        )
        .expect("header");
        assert_eq!(
            header,
            SarHeader {
                hostname: "node-01.example.com".to_string(),
                kernel: "4.18.0-553.97.1.el8_10.x86_64".to_string(),
                architecture: Some("x86_64".to_string()),
                cpu_count: Some(32),
                report_date: date(2026, 2, 21),
                ambiguous_date: false,
            }
        );
    }

    #[test]
    fn parses_short_host_without_optional_parts() {
        let header = SarHeader::parse("Linux 3.10.0 (es-01)  2026-08-05", None).expect("header");
        assert_eq!(header.hostname, "es-01");
        assert_eq!(header.architecture, None);
        assert_eq!(header.cpu_count, None);
        assert_eq!(header.report_date, date(2026, 8, 5));
    }

    #[test]
    fn rejects_non_header_lines() {
        assert!(SarHeader::parse("bash: sar: command not found", None).is_none());
        assert!(SarHeader::parse("Linuxish 1 (h) 2026-08-05", None).is_none());
        assert!(SarHeader::parse("12:00:11 AM CPU %usr", None).is_none());
    }

    #[test]
    fn unambiguous_dates_ignore_the_collection_date() {
        assert_eq!(report_date("21/02/2026", None), date(2026, 2, 21));
        assert_eq!(report_date("02/21/2026", Some(date(2026, 1, 1))), date(2026, 2, 21));
        assert_eq!(report_date("21.02.2026", None), date(2026, 2, 21));
        assert_eq!(report_date("05-Aug-2026", None), date(2026, 8, 5));
    }

    #[test]
    fn ambiguous_dates_pick_the_closest_on_or_before_collection() {
        assert_eq!(report_date("04/08/2026", Some(date(2026, 8, 5))), date(2026, 8, 4));
        assert_eq!(report_date("08/05/2026", Some(date(2026, 8, 5))), date(2026, 8, 5));
        assert_eq!(report_date("05/08/2026", Some(date(2026, 5, 9))), date(2026, 5, 8));
        assert_eq!(report_date("04/08/2026", None), date(2026, 4, 8), "falls back to MDY");

        let ambiguous = |token: &str, collected| {
            SarHeader::parse(&format!("Linux 6.1.0 (node-01) \t{token}"), collected)
                .expect("header")
                .ambiguous_date
        };
        assert!(ambiguous("04/08/2026", None));
        assert!(!ambiguous("04/08/2026", Some(date(2026, 8, 5))));
        assert!(!ambiguous("21/02/2026", None));
    }

    #[test]
    fn two_digit_years_respect_the_collection_year() {
        assert_eq!(report_date("12/31/25", Some(date(2026, 1, 1))), date(2025, 12, 31));
        assert_eq!(report_date("08/05/26", Some(date(2026, 8, 5))), date(2026, 8, 5));
        assert_eq!(report_date("12/31/99", Some(date(2000, 1, 1))), date(1999, 12, 31));
    }
}
