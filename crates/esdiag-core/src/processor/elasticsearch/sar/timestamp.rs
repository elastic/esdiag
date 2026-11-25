// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

/// A row-prefix timestamp: time-only rows take their date from the clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowTime {
    Time(NaiveTime),
    DateTime(NaiveDateTime),
}

/// Parse the timestamp that prefixes a `sar` row, returning it with the
/// number of tokens it occupies. Accepts `HH:MM:SS [AM|PM]`,
/// `YYYY-MM-DD HH:MM:SS [AM|PM]`, and `YYYY-MM-DDTHH:MM:SS`.
pub fn parse_row_time(tokens: &[&str]) -> Option<(RowTime, usize)> {
    let first = *tokens.first()?;
    if let Some((date, time)) = first.split_once('T') {
        let date = parse_date(date)?;
        let time = parse_clock(time.trim_end_matches('Z'), None)?;
        return Some((RowTime::DateTime(date.and_time(time)), 1));
    }
    if let Some(date) = parse_date(first) {
        let (time, used) = parse_time_tokens(&tokens[1..])?;
        return Some((RowTime::DateTime(date.and_time(time)), used + 1));
    }
    let (time, used) = parse_time_tokens(tokens)?;
    Some((RowTime::Time(time), used))
}

fn parse_time_tokens(tokens: &[&str]) -> Option<(NaiveTime, usize)> {
    let clock = *tokens.first()?;
    match tokens.get(1).and_then(|token| meridiem(token)) {
        Some(pm) => Some((parse_clock(clock, Some(pm))?, 2)),
        None => Some((parse_clock(clock, None)?, 1)),
    }
}

fn meridiem(token: &str) -> Option<bool> {
    if token.eq_ignore_ascii_case("AM") {
        Some(false)
    } else if token.eq_ignore_ascii_case("PM") {
        Some(true)
    } else {
        None
    }
}

fn parse_clock(clock: &str, pm: Option<bool>) -> Option<NaiveTime> {
    let mut parts = clock.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    let second: u32 = parts.next()?.split('.').next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let hour = match pm {
        None => hour,
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some(false) => hour % 12,
        Some(true) => hour % 12 + 12,
    };
    NaiveTime::from_hms_opt(hour, minute, second)
}

fn parse_date(token: &str) -> Option<NaiveDate> {
    if token.len() != 10 || token.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    NaiveDate::parse_from_str(token, "%Y-%m-%d").ok()
}

/// Dates time-only rows, advancing a day whenever the time goes backwards
/// within one section.
#[derive(Debug, Clone)]
pub struct RolloverClock {
    date: NaiveDate,
    last: Option<NaiveTime>,
}

impl RolloverClock {
    pub fn new(date: NaiveDate) -> Self {
        Self { date, last: None }
    }

    pub fn resolve(&mut self, time: RowTime) -> NaiveDateTime {
        match time {
            RowTime::DateTime(datetime) => {
                self.date = datetime.date();
                self.last = Some(datetime.time());
                datetime
            }
            RowTime::Time(time) => {
                if self.last.is_some_and(|last| time < last) {
                    self.date = self.date.succ_opt().unwrap_or(self.date);
                }
                self.last = Some(time);
                self.date.and_time(time)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(date: &str, time: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn time(value: &str) -> RowTime {
        RowTime::Time(NaiveTime::parse_from_str(value, "%H:%M:%S").unwrap())
    }

    #[test]
    fn parses_twelve_hour_rows() {
        assert_eq!(
            parse_row_time(&["12:10:10", "AM", "all", "5.71"]),
            Some((time("00:10:10"), 2))
        );
        assert_eq!(parse_row_time(&["12:10:10", "PM", "all"]), Some((time("12:10:10"), 2)));
        assert_eq!(parse_row_time(&["11:50:00", "pm"]), Some((time("23:50:00"), 2)));
    }

    #[test]
    fn parses_twenty_four_hour_rows() {
        assert_eq!(parse_row_time(&["23:50:00", "all"]), Some((time("23:50:00"), 1)));
    }

    #[test]
    fn parses_dated_rows() {
        let expected = RowTime::DateTime(at("2026-08-05", "13:10:10"));
        assert_eq!(
            parse_row_time(&["2026-08-05", "01:10:10", "PM", "all"]),
            Some((expected, 3))
        );
        assert_eq!(parse_row_time(&["2026-08-05", "13:10:10", "all"]), Some((expected, 2)));
        assert_eq!(parse_row_time(&["2026-08-05T13:10:10", "all"]), Some((expected, 1)));
    }

    #[test]
    fn rejects_non_timestamps() {
        assert_eq!(parse_row_time(&["Average:", "all", "5.71"]), None);
        assert_eq!(parse_row_time(&["Moyenne :", "all"]), None);
        assert_eq!(parse_row_time(&["Linux", "4.18.0"]), None);
        assert_eq!(parse_row_time(&["13:10:10", "PM"]), None);
        assert_eq!(parse_row_time(&[]), None);
    }

    #[test]
    fn clock_rolls_over_at_midnight() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 5).unwrap();
        let mut clock = RolloverClock::new(date);
        assert_eq!(clock.resolve(time("23:50:00")), at("2026-08-05", "23:50:00"));
        assert_eq!(clock.resolve(time("23:50:00")), at("2026-08-05", "23:50:00"));
        assert_eq!(clock.resolve(time("00:00:01")), at("2026-08-06", "00:00:01"));
        assert_eq!(clock.resolve(time("00:10:01")), at("2026-08-06", "00:10:01"));
    }

    #[test]
    fn dated_rows_set_the_clock() {
        let mut clock = RolloverClock::new(NaiveDate::from_ymd_opt(2026, 8, 5).unwrap());
        clock.resolve(RowTime::DateTime(at("2026-08-09", "10:00:00")));
        assert_eq!(clock.resolve(time("10:10:00")), at("2026-08-09", "10:10:00"));
    }
}
