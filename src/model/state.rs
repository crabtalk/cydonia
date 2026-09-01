//! What the app remembers between launches: the projects that were open, and
//! the appearance the user picked.
//!
//! Machine-written, unlike `settings.toml` — nothing here is worth hand
//! editing, and rewriting it must never cost a user their own comments.

use crate::model::settings;
use bezel::theme::{TextStyle, appearance::AppearanceMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub appearance: AppearanceMode,
    /// Whether the frost is off — bezel paints opaque surfaces instead.
    #[serde(default)]
    pub reduce_transparency: bool,
    /// Whether the text caret blinks. Off holds it lit.
    pub cursor_blink: bool,
    /// The body size the type ladder is scaled against, in points.
    pub text_size: f32,
    /// The greys' oklch hue in degrees, and how much of it they carry. Zero
    /// chroma is the shipped neutral, whatever the hue says.
    pub hue: f32,
    pub chroma: f32,
}

/// What the body size may be set to, in points: the ladder's smallest measured
/// role to Title3's, so bezel's fixed chrome heights hold at either end. Read
/// on the way in as well as by the control, because a size out of range paints
/// an interface nobody can read the settings window to fix.
pub const TEXT_SIZE: (f32, f32) = (11., 17.);

/// Hand-written because a zeroed `text_size` is a font nobody can read, and a
/// missing state file resolves every field through here.
impl Default for State {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            active: 0,
            appearance: AppearanceMode::default(),
            reduce_transparency: false,
            cursor_blink: true,
            text_size: TextStyle::Body.size(),
            hue: 0.,
            chroma: 0.,
        }
    }
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
        reduce_transparency: stored.reduce_transparency,
        cursor_blink: stored.cursor_blink,
        text_size: stored.text_size.clamp(TEXT_SIZE.0, TEXT_SIZE.1),
        hue: stored.hue,
        chroma: stored.chroma,
    }
}

/// Best effort: a state file that cannot be written is not worth failing a
/// click over.
pub fn save(state: &State) {
    let Some(path) = path() else {
        return;
    };
    if let Ok(body) = toml::to_string_pretty(state)
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&path, body);
    }
}
