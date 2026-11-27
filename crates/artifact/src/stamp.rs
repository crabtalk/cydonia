//! The millisecond an entry is named for, and the one it last changed at.

use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};
#[cfg(feature = "fs")]
use std::{path::Path, time::UNIX_EPOCH};

static CLOCK: OnceLock<fn() -> u64> = OnceLock::new();

/// The last stamp [`fresh`] handed out in this process.
static LAST: AtomicU64 = AtomicU64::new(0);

/// Now, in whole seconds.
pub fn seconds() -> u64 {
    u64::try_from(now() / 1000).unwrap_or(u64::MAX)
}

/// Install a host clock in Unix milliseconds. Featureless bare Wasm guests
/// must install it before using operations that mint timestamps.
pub fn set_clock(clock: fn() -> u64) -> Result<(), fn() -> u64> {
    CLOCK.set(clock)
}

pub fn now() -> u128 {
    if let Some(clock) = CLOCK.get() {
        return u128::from(clock());
    }
    #[cfg(not(feature = "fs"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(feature = "fs")]
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

/// `stamp` as an id, or the first stamp after it that `taken` does not hold.
pub fn free(stamp: u128, taken: impl Fn(&str) -> bool) -> String {
    (stamp..)
        .map(|stamp| stamp.to_string())
        .find(|id| !taken(id))
        .unwrap_or_else(|| stamp.to_string())
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

/// The stamp a backend named an entry for, read back out of that name. `None`
/// for a name that is not one.
pub fn named(name: &str) -> Option<u128> {
    name.parse().ok()
}

/// When a file was last written, as the same stamp ids carry — the key entries
/// are listed by, so the one you touched last is the one on top.
#[cfg(feature = "fs")]
pub fn of(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(now, |since| since.as_millis())
}
