//! amber_humantime — Replacement for `humantime`
//!
//! Formats durations and timestamps in human-readable form.

use std::time::Duration;

/// Format a duration in human-readable form
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format a duration with precision
pub fn format_duration_precise(d: Duration) -> String {
    let micros = d.as_micros();
    if micros < 1_000 {
        format!("{}µs", micros)
    } else if micros < 1_000_000 {
        format!("{:.1}ms", micros as f64 / 1_000.0)
    } else {
        format_duration(d)
    }
}

/// Parse a human-readable duration
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (num_str, unit) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(s.len()),
    );
    let num: f64 = num_str.parse().ok()?;
    let secs = match unit.trim() {
        "s" | "sec" | "secs" | "second" | "seconds" => num as u64,
        "m" | "min" | "mins" | "minute" | "minutes" => (num * 60.0) as u64,
        "h" | "hr" | "hrs" | "hour" | "hours" => (num * 3600.0) as u64,
        "d" | "day" | "days" => (num * 86400.0) as u64,
        "w" | "wk" | "week" | "weeks" => (num * 604800.0) as u64,
        "ms" | "milli" | "millis" => (num / 1000.0) as u64,
        "us" | "micro" | "micros" => (num / 1_000_000.0) as u64,
        "" => num as u64,
        _ => return None,
    };
    Some(Duration::from_secs(secs))
}

