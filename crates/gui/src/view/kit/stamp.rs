//! An entry's stamps as the app shows them.

use chrono::Datelike as _;

/// A millisecond stamp as coarse as still says something: the time today, the
/// day this year, the date before that. Empty for none.
pub(crate) fn coarse(ms: u128) -> String {
    let Some(at) = local(ms) else {
        return String::new();
    };
    let now = chrono::Local::now();
    let format = match () {
        _ if at.date_naive() == now.date_naive() => "%-I:%M %p",
        _ if at.year() == now.year() => "%b %-d",
        _ => "%b %-d, %Y",
    };
    at.format(format).to_string()
}

/// A millisecond stamp in words a sentence can carry: "today at 2:14 PM",
/// "yesterday at 9:03 AM", "Oct 3, 2026". Empty for none.
pub(crate) fn full(ms: u128) -> String {
    let Some(at) = local(ms) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    let day = at.date_naive();
    match () {
        _ if day == today => at.format("today at %-I:%M %p").to_string(),
        _ if today.pred_opt() == Some(day) => at.format("yesterday at %-I:%M %p").to_string(),
        _ => at.format("%b %-d, %Y").to_string(),
    }
}

/// How long ago a millisecond stamp was, as short as a row's corner holds:
/// `now`, `15m`, `3h`, `2d` within the week, then the day this year and the
/// date before that. Empty for none.
pub(crate) fn ago(ms: u128) -> String {
    let Some(at) = local(ms) else {
        return String::new();
    };
    let now = chrono::Local::now();
    let since = now.signed_duration_since(at);
    match () {
        _ if since.num_minutes() < 1 => "now".to_owned(),
        _ if since.num_hours() < 1 => format!("{}m", since.num_minutes()),
        _ if since.num_days() < 1 => format!("{}h", since.num_hours()),
        _ if since.num_days() < 7 => format!("{}d", since.num_days()),
        _ if at.year() == now.year() => at.format("%b %-d").to_string(),
        _ => at.format("%b %-d, %Y").to_string(),
    }
}

/// A millisecond stamp in local time; none for zero or out of range.
fn local(ms: u128) -> Option<chrono::DateTime<chrono::Local>> {
    i64::try_from(ms)
        .ok()
        .filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|at| at.with_timezone(&chrono::Local))
}
