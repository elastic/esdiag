// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

/// ANSI CSI sequences, character-set designations, any other escape
/// sequence's introducer, carriage returns, and NUL bytes.
static CONTROL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b[()][0-9A-Za-z]|\x1b[@-_]?|[\r\x00]").expect("control regex")
});

/// Remove terminal decoration and control bytes, keeping column spacing.
pub fn clean_line(line: &str) -> Cow<'_, str> {
    if line.bytes().any(|byte| byte == 0x1b || byte == b'\r' || byte == 0) {
        CONTROL.replace_all(line, "")
    } else {
        Cow::Borrowed(line)
    }
}

#[cfg(test)]
mod tests {
    use super::clean_line;

    #[test]
    fn strips_sgr_sequences() {
        assert_eq!(
            clean_line("\x1b[1;31m12:10:10 AM\x1b[0m     all      5.71"),
            "12:10:10 AM     all      5.71"
        );
    }

    #[test]
    fn strips_cursor_and_erase_sequences() {
        assert_eq!(
            clean_line("\x1b[2K\x1b[1G12:10:10 AM  \x1b[?25lCPU"),
            "12:10:10 AM  CPU"
        );
    }

    #[test]
    fn strips_mixed_control_bytes_and_keeps_spacing() {
        assert_eq!(
            clean_line("12:10:10 AM\x00     all\r      5.71\x1b(B\r"),
            "12:10:10 AM     all      5.71"
        );
    }

    #[test]
    fn clean_lines_are_borrowed() {
        assert!(matches!(
            clean_line("12:10:10 AM  all  5.71"),
            std::borrow::Cow::Borrowed(_)
        ));
    }
}
