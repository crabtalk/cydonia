//! An entry's stamps as the app shows them.

use chrono::Datelike as _;

/// A millisecond stamp as coarse as still says something: the time today, the
/// day this year, the date before that. Empty for none.
pub(crate) fn coarse(ms: u128) -> String {
    let Some(at) = i64::try_from(ms)
        .ok()
        .filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
    else {
        return String::new();
    };
    let at = at.with_timezone(&chrono::Local);
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
    let Some(at) = i64::try_from(ms)
        .ok()
        .filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
    else {
        return String::new();
    };
    let at = at.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let day = at.date_naive();
    match () {
        _ if day == today => at.format("today at %-I:%M %p").to_string(),
        _ if today.pred_opt() == Some(day) => at.format("yesterday at %-I:%M %p").to_string(),
        _ => at.format("%b %-d, %Y").to_string(),
    }
}
