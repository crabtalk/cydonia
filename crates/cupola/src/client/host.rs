//! A saved host's id, which outlives the controller that issued it: written
//! in its decimal form wherever an embedder keeps a reference to the host.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A saved host's id. Never given to another host of the same hosts file,
/// removed or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostId(u64);

/// Text that is not a [`HostId`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a host id")]
pub struct NotHostId(String);

#[cfg(unix)]
impl HostId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Reads what `Display` writes: decimal digits, and nothing else.
impl FromStr for HostId {
    type Err = NotHostId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
        digits
            .then(|| text.parse().ok())
            .flatten()
            .map(Self)
            .ok_or_else(|| NotHostId(text.to_owned()))
    }
}
