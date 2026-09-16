//! Human-friendly formatting helpers: byte sizes, counts, percentages,
//! durations, timestamps and width-aware string fitting.

use std::time::Duration;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const BINARY_UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
const SI_UNITS: [&str; 7] = ["B", "kB", "MB", "GB", "TB", "PB", "EB"];

/// Format a byte count as a `(number, unit)` pair, e.g. `("12.4", "GiB")`.
/// The number is at most 5 characters wide so callers can right-align it.
pub fn bytes(n: u64, si: bool) -> (String, &'static str) {
    let (base, units) = if si {
        (1000.0, &SI_UNITS)
    } else {
        (1024.0, &BINARY_UNITS)
    };
    if n < 1000 && (si || n < 1024) {
        return (n.to_string(), units[0]);
    }
    let mut value = n as f64;
    let mut idx = 0;
    while value >= base && idx < units.len() - 1 {
        value /= base;
        idx += 1;
    }
    let text = if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    (text, units[idx])
}

/// Combined size string, e.g. `12.4 GiB`.
pub fn bytes_str(n: u64, si: bool) -> String {
    let (v, u) = bytes(n, si);
    format!("{v} {u}")
}

/// Thousands-separated integer, e.g. `45,231`.
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Percentage with one decimal, e.g. `61.2%`.
pub fn percent(share: f64) -> String {
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    format!("{:.1}%", share * 100.0)
}

/// Short elapsed-time string: `0.8s`, `12.3s`, `2m 05s`, `1h 02m`.
pub fn duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else if secs < 3600.0 {
        let m = (secs / 60.0).floor();
        let s = secs - m * 60.0;
        format!("{m:.0}m {s:02.0}s")
    } else {
        let h = (secs / 3600.0).floor();
        let m = ((secs - h * 3600.0) / 60.0).floor();
        format!("{h:.0}h {m:02.0}m")
    }
}

/// Format a unix timestamp (seconds) using a chrono format string in local time.
pub fn mtime(secs: i64, fmt: &str) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(secs, 0).single() {
        Some(t) => t.format(fmt).to_string(),
        None => "-".to_string(),
    }
}

/// Display width of a string in terminal cells.
pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate `s` on the right so it fits `max` cells, appending `…` if cut.
pub fn truncate_right(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Truncate `s` on the left so it fits `max` cells, prepending `…` if cut.
pub fn truncate_left(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 0;
    for ch in s.chars().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max - 1 {
            break;
        }
        kept.push(ch);
        used += w;
    }
    let mut out = String::from("…");
    out.extend(kept.iter().rev());
    out
}

/// Truncate on the right and pad with spaces so the result is exactly `max` cells wide.
pub fn fit(s: &str, max: usize) -> String {
    let mut out = truncate_right(s, max);
    let w = width(&out);
    if w < max {
        out.extend(std::iter::repeat_n(' ', max - w));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_binary() {
        assert_eq!(bytes(0, false), ("0".into(), "B"));
        assert_eq!(bytes(1023, false), ("1023".into(), "B"));
        assert_eq!(bytes(1024, false), ("1.0".into(), "KiB"));
        assert_eq!(bytes(1536, false), ("1.5".into(), "KiB"));
        assert_eq!(bytes(12_400_000_000, false), ("11.5".into(), "GiB"));
        assert_eq!(bytes(200 * 1024 * 1024, false), ("200".into(), "MiB"));
    }

    #[test]
    fn bytes_si() {
        assert_eq!(bytes(999, true), ("999".into(), "B"));
        assert_eq!(bytes(1000, true), ("1.0".into(), "kB"));
        assert_eq!(bytes(12_400_000_000, true), ("12.4".into(), "GB"));
    }

    #[test]
    fn count_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(45231), "45,231");
        assert_eq!(count(1_234_567_890), "1,234,567,890");
    }

    #[test]
    fn percent_clamps() {
        assert_eq!(percent(0.612), "61.2%");
        assert_eq!(percent(f64::NAN), "0.0%");
        assert_eq!(percent(2.0), "100.0%");
    }

    #[test]
    fn duration_forms() {
        assert_eq!(duration(Duration::from_millis(800)), "0.8s");
        assert_eq!(duration(Duration::from_secs(125)), "2m 05s");
        assert_eq!(duration(Duration::from_secs(3720)), "1h 02m");
    }

    #[test]
    fn truncation_is_width_aware() {
        assert_eq!(truncate_right("hello", 10), "hello");
        assert_eq!(truncate_right("hello world", 6), "hello…");
        assert_eq!(truncate_left("/a/b/c/d", 5), "…/c/d");
        // Wide characters count as two cells.
        assert_eq!(truncate_right("📁📁📁", 5), "📁📁…");
        assert_eq!(width(&fit("📁x", 6)), 6);
    }
}
