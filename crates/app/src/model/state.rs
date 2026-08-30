//! What the app remembers between launches: the projects that were open, and
//! the appearance the user picked.
//!
//! Machine-written, unlike `settings.toml` — nothing here is worth hand
//! editing, and rewriting it must never cost a user their own comments.

use crate::model::{project::Project, settings};
use bezel::theme::appearance::AppearanceMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub appearance: AppearanceMode,
}

fn path() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("state.toml"))
}

/// Paths that have since vanished are dropped — a renamed folder would
/// otherwise leave a tab no agent can spawn in. The active project is resolved
/// by path first, so dropping an earlier one doesn't shift it.
pub fn restore() -> State {
    let stored: State = path()
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
    State {
        projects,
        active,
        appearance: stored.appearance,
    }
}

/// Best effort: a state file that cannot be written is not worth failing a
/// click over.
pub fn save(projects: &[Project], active: Option<usize>, appearance: AppearanceMode) {
    let Some(path) = path() else {
        return;
    };
    let state = State {
        projects: projects
            .iter()
            .map(|project| project.path.clone())
            .collect(),
        active: active.unwrap_or_default(),
        appearance,
    };
    if let Ok(body) = toml::to_string_pretty(&state)
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&path, body);
    }
}
