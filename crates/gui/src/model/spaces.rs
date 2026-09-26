//! Where spaces are kept: `~/.config/cydonia/spaces/`, beside the state file
//! rather than inside any project.
//!
//! A space spans projects — a pane on a session in one repo beside a pane on
//! a board in another — so it belongs to no single `.cydonia/`. What it holds
//! is absolute paths on this machine, which is the same reason
//! [`crate::model::state`] lives here: neither is worth carrying to another
//! machine, and neither is the project's source.
//!
//! One file per space, named for the millisecond it was made, the way boards
//! are inside a project. Best effort throughout: a space that cannot be
//! written is not worth failing a drag over.
//!
//! Without the `desktop` feature there is no config directory, and spaces are
//! held in memory for as long as the process runs.

#[cfg(feature = "desktop")]
use crate::model::settings;
use artifact::{space::Space, stamp};
use std::collections::HashSet;
#[cfg(feature = "desktop")]
use std::path::PathBuf;

/// `~/.config/cydonia/spaces/`.
#[cfg(feature = "desktop")]
fn dir() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("spaces"))
}

#[cfg(feature = "desktop")]
fn file(id: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("{id}.toml")))
}

/// Every space on this machine, most recently written first.
#[cfg(feature = "desktop")]
pub fn all() -> Vec<Space> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut spaces: Vec<Space> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| {
            let body = std::fs::read_to_string(&path).ok()?;
            let mut space: Space = toml::from_str(&body).ok()?;
            space.touched = stamp::of(&path);
            if space.id.is_empty() {
                space.id = path.file_stem()?.to_str()?.to_owned();
            }
            Some(space)
        })
        .collect();
    spaces.sort_by_key(|space| std::cmp::Reverse(space.touched));
    spaces
}

/// Read one back, for a test or a re-read.
#[cfg(feature = "desktop")]
pub fn read(id: &str) -> Option<Space> {
    let path = file(id)?;
    let body = std::fs::read_to_string(&path).ok()?;
    let mut space: Space = toml::from_str(&body).ok()?;
    space.touched = stamp::of(&path);
    if space.id.is_empty() {
        space.id = id.to_owned();
    }
    Some(space)
}

/// Mint one, named clear of those already here.
#[cfg(feature = "desktop")]
pub fn create(name: &str, over: artifact::space::Member) -> Option<Space> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let taken: HashSet<String> = all().into_iter().map(|space| space.name).collect();
    let name = match name.is_empty() {
        false => name.to_owned(),
        true => artifact::space::next_name(&taken),
    };
    // This millisecond's file, or the first after it that is not taken — two
    // made inside one millisecond is the only way that happens.
    let id = (stamp::now()..)
        .map(|stamp| stamp.to_string())
        .find(|id| !dir.join(format!("{id}.toml")).exists())?;
    let mut space = Space::new(id, &name, over);
    save(&mut space);
    Some(space)
}

/// Write one back, and take the time it was written at — what the sidebar
/// orders on.
#[cfg(feature = "desktop")]
pub fn save(space: &mut Space) {
    let (Some(path), Ok(body)) = (file(&space.id), toml::to_string_pretty(&*space)) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(&path, body).is_ok() {
        space.touched = stamp::now();
    }
}

#[cfg(feature = "desktop")]
pub fn remove(id: &str) {
    if let Some(path) = file(id) {
        let _ = std::fs::remove_file(path);
    }
}

// ── without the `desktop` feature ─────────────────────────────────
//
// No config directory: spaces are held for as long as the process runs.

#[cfg(not(feature = "desktop"))]
fn held() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<String, Space>> {
    static HELD: std::sync::Mutex<std::collections::BTreeMap<String, Space>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(not(feature = "desktop"))]
pub fn all() -> Vec<Space> {
    let mut spaces: Vec<Space> = held().values().cloned().collect();
    spaces.sort_by_key(|space| std::cmp::Reverse(space.touched));
    spaces
}

#[cfg(not(feature = "desktop"))]
pub fn read(id: &str) -> Option<Space> {
    held().get(id).cloned()
}

#[cfg(not(feature = "desktop"))]
pub fn create(name: &str, over: artifact::space::Member) -> Option<Space> {
    let taken: HashSet<String> = all().into_iter().map(|space| space.name).collect();
    let name = match name.is_empty() {
        false => name.to_owned(),
        true => artifact::space::next_name(&taken),
    };
    let id = (stamp::now()..)
        .map(|stamp| stamp.to_string())
        .find(|id| !held().contains_key(id))?;
    let mut space = Space::new(id, &name, over);
    save(&mut space);
    Some(space)
}

#[cfg(not(feature = "desktop"))]
pub fn save(space: &mut Space) {
    space.touched = stamp::now();
    held().insert(space.id.clone(), space.clone());
}

#[cfg(not(feature = "desktop"))]
pub fn remove(id: &str) {
    held().remove(id);
}
