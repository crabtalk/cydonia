//! A release's version, as the feed, its tag and `state.toml` spell it.

use serde::{Deserialize, Serialize};

/// A release's version as the feed and its tag spell it: `0.1.4`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(String);

impl Version {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// This build's.
    pub fn running() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a release to move to from `running`. Only versions of
    /// three numbers are ordered; either side being anything else, a
    /// pre-release tag or a build suffix, is never newer.
    pub fn newer_than(&self, running: &Self) -> bool {
        match (self.numbers(), running.numbers()) {
            (Some(offered), Some(running)) => offered > running,
            _ => false,
        }
    }

    /// Major, minor and patch. `None` for anything but three numbers.
    pub(crate) fn numbers(&self) -> Option<[u64; 3]> {
        let mut parts = self.0.split('.');
        let mut out = [0; 3];
        for slot in &mut out {
            *slot = parts.next()?.parse().ok()?;
        }
        parts.next().is_none().then_some(out)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
