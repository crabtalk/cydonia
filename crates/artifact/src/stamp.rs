//! The millisecond an entry is named for, and the one it last changed at.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// The last stamp [`fresh`] handed out in this process.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Now.
pub fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

/// Now, or one past the last stamp this returned, whichever is later: no two
/// calls in one process answer the same stamp.
pub fn fresh() -> u128 {
    let now = u64::try_from(now()).unwrap_or(u64::MAX);
    let mut last = LAST.load(Ordering::Relaxed);
    loop {
        let next = now.max(last + 1);
        match LAST.compare_exchange_weak(last, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return u128::from(next),
            Err(seen) => last = seen,
        }
    }
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
