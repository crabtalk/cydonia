//! Small transforms that belong to no type.

use std::time::SystemTime;

/// How long ago, at a glance. Coarse on purpose: the rail is a list, not a
/// clock, and a second line that changed every second would be the loudest
/// thing on it.
pub fn ago(at: SystemTime) -> String {
    let Ok(since) = at.elapsed() else {
        return "just now".to_owned();
    };
    let secs = since.as_secs();
    match secs {
        ..60 => "just now".to_owned(),
        60..3_600 => format!("{}m ago", secs / 60),
        3_600..86_400 => format!("{}h ago", secs / 3_600),
        _ => format!("{}d ago", secs / 86_400),
    }
}
