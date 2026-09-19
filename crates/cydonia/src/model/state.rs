//! What the app remembers between launches: which projects were open, which
//! was in front, and what each of them was last showing.
//!
//! Bookkeeping and nothing else. Machine-written, unlike `settings.toml` —
//! nothing here is worth hand editing, and rewriting it must never cost a user
//! their own comments. That is also why the preferences that used to sit here
//! no longer do: they are worth editing, and worth carrying to another
//! machine, which the absolute paths below are not. See
//! [`crate::model::settings::Appearance`], and [`crate::model::migrate`] for
//! the move.

use crate::model::settings;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// The four things a project holds. Which one a launch lands on is the last
/// one that was open, so the window comes back where it was left.
///
/// A layout is not among them: it spans projects and is kept beside this file
/// rather than in any of them — see [`crate::model::layouts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Session,
    Board,
    Article,
    Table,
}

/// One remembered entry: which kind, and which of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    /// The file the entry is, or a table's key. An index would drift as
    /// siblings are added and removed between launches.
    pub id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    #[serde(default)]
    pub projects: Vec<PathBuf>,
    #[serde(default)]
    pub active: usize,
    /// Projects whose sidebar children are hidden.
    #[serde(default)]
    pub collapsed: Vec<PathBuf>,
    /// What each project was last showing, by project path. Last in the struct
    /// because a map renders as TOML tables, and a bare key after one of those
    /// belongs to it.
    #[serde(default)]
    pub last: BTreeMap<PathBuf, Entry>,
    /// The order a project's entries are listed in, by project path — see
    /// [`crate::model::workspace`]'s `order`.
    ///
    /// An entry the list does not name is one made since it was last written,
    /// and is listed above everything here. Names that no longer resolve are
    /// left alone: an entry deleted on another checkout of the same path is
    /// one a branch may bring back, and a stale name costs a lookup that
    /// already has to miss.
    #[serde(default)]
    pub order: BTreeMap<PathBuf, Vec<Entry>>,
    /// The entries held at the top of each project's list, by project path.
    /// A list rather than a flag on [`Entry`]: pins are ordered among
    /// themselves, and `last` has no use for one.
    #[serde(default)]
    pub pinned: BTreeMap<PathBuf, Vec<Entry>>,
    /// The layout the window was showing when it last closed, by its id — see
    /// [`crate::model::layouts`]. Nothing where it was on a single entry, and
    /// an id whose file has since gone lands on one too.
    #[serde(default)]
    pub layout: Option<String>,
}

/// `~/.config/cydonia/state.toml`, beside the settings it is not.
pub(crate) fn path() -> Option<PathBuf> {
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
        collapsed: stored.collapsed,
        last: stored.last,
        order: stored.order,
        pinned: stored.pinned,
        layout: stored.layout,
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
