//! A session on disk: the transcript, and the agent's own id for it.
//!
//! Beside the articles in the project's own `.cydonia/`, because a session is
//! the project's record the same way an article is — and one file each, so
//! writing one does not rewrite the rest.
//!
//! Written as the session changes rather than when it ends, so a session
//! survives a crash and not just an orderly quit.

use crate::model::{project, session::ChatItem};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
pub struct Record {
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

const SESSIONS: &str = "sessions";

fn dir(project: &Path) -> PathBuf {
    project::dir(project).join(SESSIONS)
}

/// Every session filed in the project with the file it came from, oldest
/// first. The path is what lets a row rewrite or delete itself later.
pub fn list(project: &Path) -> Vec<(PathBuf, Record)> {
    let Ok(entries) = std::fs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Record)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let body = std::fs::read_to_string(&path).ok()?;
            Some((path, serde_json::from_str(&body).ok()?))
        })
        .collect();
    found.sort_by_key(|(_, record)| record.updated);
    found
}

/// Mint the file a session is written to from here on. Called on the first
/// write and not before: opening a project must not put a `.cydonia/` in it.
pub fn create(project: &Path) -> Option<PathBuf> {
    let dir = project::init(project).ok()?.join(SESSIONS);
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = project::stamp();
    let mut path = dir.join(format!("{stamp}.json"));
    for n in 2.. {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{stamp}-{n}.json"));
    }
    Some(path)
}

/// Best effort, like [`remove`]: a session that cannot be written is not worth
/// failing a turn over.
pub fn write(file: &Path, record: &Record) {
    if let Ok(body) = serde_json::to_string_pretty(record) {
        let _ = std::fs::write(file, body);
    }
}

pub fn remove(file: &Path) {
    let _ = std::fs::remove_file(file);
}
