//! The millisecond an entry is named for, and the one it last changed at.

use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Now.
pub fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

/// When a file was last written, as the same stamp ids carry — the key entries
/// are listed by, so the one you touched last is the one on top.
pub fn of(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(now, |since| since.as_millis())
}
