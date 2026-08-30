//! Archived sessions: the transcripts that outlive the process.
//!
//! Beside the articles in the project's own `.cydonia/`, because a transcript
//! is the project's record the same way an article is — and one file each, so
//! archiving one session does not rewrite the rest.
//!
//! The record carries no id. Session ids are minted per launch and mean
//! nothing across one; a reloaded transcript is given a fresh one like any
//! other session.

use crate::model::session::ChatItem;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
pub struct Archived {
    /// The agent it ran on, by the name `settings.toml` gives it. Enough to
    /// label the row and find its mark; not enough to resume, which this does
    /// not do.
    pub agent: String,
    pub title: String,
    pub name: Option<String>,
    /// Seconds since the epoch — `SystemTime` has no serialization of its own,
    /// and this file is read by a later build than wrote it.
    pub updated: u64,
    pub items: Vec<ChatItem>,
}

impl Archived {
    pub fn at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.updated)
    }
}

fn dir(project: &Path) -> PathBuf {
    project.join(".cydonia").join("sessions")
}

/// Every archived session in the project with the file it came from, oldest
/// first. The path is what lets a row delete itself later.
pub fn list(project: &Path) -> Vec<(PathBuf, Archived)> {
    let Ok(entries) = std::fs::read_dir(dir(project)) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Archived)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let body = std::fs::read_to_string(&path).ok()?;
            Some((path, serde_json::from_str(&body).ok()?))
        })
        .collect();
    found.sort_by_key(|(_, archived)| archived.updated);
    found
}

/// File it, and say where. The name is the timestamp the list is ordered by,
/// with a counter for the second one to land inside the same second.
pub fn write(project: &Path, archived: &Archived) -> Option<PathBuf> {
    let dir = dir(project);
    std::fs::create_dir_all(&dir).ok()?;
    let mut path = dir.join(format!("{}.json", archived.updated));
    for n in 2.. {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{}-{n}.json", archived.updated));
    }
    let body = serde_json::to_string_pretty(archived).ok()?;
    std::fs::write(&path, body).ok()?;
    Some(path)
}

/// Best effort: a file that cannot be deleted is not worth failing a click
/// over, and the row is going either way.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}
