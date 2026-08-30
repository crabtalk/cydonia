//! A project: a directory, the sessions running in it, and its board.
//!
//! The path is the whole identity — it is what every session in the project
//! is spawned with as its `cwd`, and what [`crate::model::state`] persists.

use crate::model::{
    board::{self, Board},
    session::ChatSession,
};
use std::path::PathBuf;

pub struct Project {
    pub path: PathBuf,
    pub sessions: Vec<ChatSession>,
    pub active: Option<u64>,
    pub board: Board,
}

impl Project {
    pub fn new(path: PathBuf) -> Self {
        Self {
            board: board::load(&path),
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
