//! A project: a directory, and the sessions running in it.
//!
//! The path is the whole identity — it is what every session in the project
//! is spawned with as its `cwd`, and what survives a restart.

use crate::{session::ChatSession, settings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Project {
    pub path: PathBuf,
    pub sessions: Vec<ChatSession>,
    pub active: Option<u64>,
}

impl Project {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            sessions: Vec::new(),
            active: None,
        }
    }

    /// The tab's label: the directory's own name, or the whole path when it
    /// has none (`/`).
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.sessions.iter().find(|chat| chat.id == id)
    }

    pub fn session_mut(&mut self, id: u64) -> Option<&mut ChatSession> {
        self.sessions.iter_mut().find(|chat| chat.id == id)
    }

    pub fn active_session(&self) -> Option<&ChatSession> {
        self.active.and_then(|id| self.session(id))
    }
}

/// The open tabs, and which one was in front. App state rather than user
/// config, but it lives beside `settings.toml` — one file does not earn a
/// directory of its own.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    #[serde(default)]
    pub active: usize,
}

fn state_path() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("state.toml"))
}

/// The tabs to reopen. Paths that have since vanished are dropped — a renamed
/// folder would otherwise leave a tab no agent can spawn in. The active tab is
/// resolved by path first, so dropping an earlier one doesn't shift it.
pub fn restore() -> State {
    let stored: State = state_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| toml::from_str(&body).ok())
        .unwrap_or_default();
    let active = stored.projects.get(stored.active).cloned();
    let projects: Vec<PathBuf> = stored
        .projects
        .into_iter()
        .filter(|path| path.is_dir())
        .collect();
    let active = active
        .and_then(|path| projects.iter().position(|open| *open == path))
        .unwrap_or(0);
    State { projects, active }
}

/// Best effort: a state file that cannot be written is not worth failing a
/// click over.
pub fn save(projects: &[Project], active: Option<usize>) {
    let Some(path) = state_path() else {
        return;
    };
    let state = State {
        projects: projects
            .iter()
            .map(|project| project.path.clone())
            .collect(),
        active: active.unwrap_or_default(),
    };
    if let Ok(body) = toml::to_string_pretty(&state)
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&path, body);
    }
}
