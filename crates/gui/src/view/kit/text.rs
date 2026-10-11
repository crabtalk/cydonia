//! Numbers and names as the app writes them in a line of text.

use std::time::Duration;

/// `n` with its thousands grouped: `12,480`.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (ix, digit) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Tokens, thinned to the digits that carry: `840`, `4.2K`, `128K`, `1.2M`,
/// `3.40B`.
pub(crate) fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..100_000 => format!("{:.1}K", n as f64 / 1e3),
        100_000..1_000_000 => format!("{}K", n / 1_000),
        1_000_000..100_000_000 => format!("{:.1}M", n as f64 / 1e6),
        100_000_000..1_000_000_000 => format!("{}M", n / 1_000_000),
        _ => format!("{:.2}B", n as f64 / 1e9),
    }
}

/// A running time: `42s`, then `03m 07s`.
pub(crate) fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds < 60 {
        true => format!("{seconds}s"),
        false => format!("{:02}m {:02}s", seconds / 60, seconds % 60),
    }
}

/// `title`, or `fallback` when it is blank.
pub(crate) fn untitled(title: &str, fallback: &str) -> String {
    match title.trim() {
        "" => fallback.to_owned(),
        title => title.to_owned(),
    }
}
