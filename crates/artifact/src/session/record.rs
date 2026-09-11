//! What a session is filed as: what it ran on, what it is called, and the
//! transcript it got to.
//!
//! Written as the session changes rather than when it ends, so a session
//! survives a crash and not just an orderly quit.

use crate::session::chat::ChatItem;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub struct Record {
    /// What names this session here, minted when its file is and never moving
    /// after. Not [`Record::session`]: that one is the agent's, absent until
    /// the session first reaches one, and gone the moment the agent forgets
    /// it. This is ours, and it is what a reader asks for a session by.
    ///
    /// Defaulted, and filled from the file's own name for a session written
    /// before ids existed — see [`crate::id`].
    #[serde(default)]
    pub id: String,
    /// The agent it runs on, by the name `settings.toml` gives it. Resolving
    /// that name against the settings is what lets the session reconnect.
    pub agent: String,
    /// The agent's own id for the session, which is what `session/load`
    /// resumes. Absent when the session never reached an agent.
    #[serde(default)]
    pub session: Option<String>,
    pub title: String,
    pub name: Option<String>,
    /// Seconds since the epoch — `SystemTime` has no serialization of its own,
    /// and this file is read by a later build than wrote it.
    pub updated: u64,
    /// Whether the user archived it. A closed session sinks below the ones
    /// still going on, and typing into it brings it back.
    #[serde(default)]
    pub closed: bool,
    pub items: Vec<ChatItem>,
}

impl Record {
    pub fn at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.updated)
    }
}
