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
use artifact::{
    space::{Member, Space},
    stamp,
};
use std::collections::HashSet;
#[cfg(feature = "desktop")]
use std::path::PathBuf;

/// Where this build keeps spaces.
#[cfg(feature = "desktop")]
pub type Shelf = Disk;
#[cfg(not(feature = "desktop"))]
pub type Shelf = Memory;

/// What keeps spaces.
pub trait Store {
    /// Every space held, most recently written first.
    fn all() -> Vec<Space>;

    fn read(id: &str) -> Option<Space>;

    /// Write one back, and take the time it was written at — what [`Self::all`]
    /// orders on.
    fn save(space: &mut Space);

    fn remove(id: &str);

    /// Whether a space is held under `id`.
    fn holds(id: &str) -> bool;

    /// Ready to take a new space; `false` where nothing can be written.
    fn prepare() -> bool;

    /// Mint one, named clear of those already here.
    fn create(over: Member) -> Option<Space> {
        if !Self::prepare() {
            return None;
        }
        let taken: HashSet<String> = Self::all().into_iter().map(|space| space.name).collect();
        let name = artifact::space::next_name(&taken);
        // This millisecond, or the first after it that is not taken — two made
        // inside one millisecond is the only way that happens.
        let id = (stamp::now()..)
            .map(|stamp| stamp.to_string())
            .find(|id| !Self::holds(id))?;
        let mut space = Space::new(id, &name, over);
        Self::save(&mut space);
        Some(space)
    }
}

/// `~/.config/cydonia/spaces/`, one file per space.
#[cfg(feature = "desktop")]
pub struct Disk;

#[cfg(feature = "desktop")]
impl Disk {
    fn dir() -> Option<PathBuf> {
        settings::dir().ok().map(|dir| dir.join("spaces"))
    }

    fn file(id: &str) -> Option<PathBuf> {
        Some(Self::dir()?.join(format!("{id}.toml")))
    }
}

#[cfg(feature = "desktop")]
impl Store for Disk {
    fn all() -> Vec<Space> {
        let Some(dir) = Self::dir() else {
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

    fn read(id: &str) -> Option<Space> {
        let path = Self::file(id)?;
        let body = std::fs::read_to_string(&path).ok()?;
        let mut space: Space = toml::from_str(&body).ok()?;
        space.touched = stamp::of(&path);
        if space.id.is_empty() {
            space.id = id.to_owned();
        }
        Some(space)
    }

    fn save(space: &mut Space) {
        let Some(path) = Self::file(&space.id) else {
            return;
        };
        if settings::write_toml(&path, &*space).is_ok() {
            space.touched = stamp::now();
        }
    }

    fn remove(id: &str) {
        if let Some(path) = Self::file(id) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn holds(id: &str) -> bool {
        Self::file(id).is_some_and(|path| path.exists())
    }

    fn prepare() -> bool {
        Self::dir().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
    }
}

/// Held for as long as the process runs.
#[cfg(not(feature = "desktop"))]
pub struct Memory;

#[cfg(not(feature = "desktop"))]
impl Memory {
    fn held() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<String, Space>> {
        static HELD: std::sync::Mutex<std::collections::BTreeMap<String, Space>> =
            std::sync::Mutex::new(std::collections::BTreeMap::new());
        HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(not(feature = "desktop"))]
impl Store for Memory {
    fn all() -> Vec<Space> {
        let mut spaces: Vec<Space> = Self::held().values().cloned().collect();
        spaces.sort_by_key(|space| std::cmp::Reverse(space.touched));
        spaces
    }

    fn read(id: &str) -> Option<Space> {
        Self::held().get(id).cloned()
    }

    fn save(space: &mut Space) {
        space.touched = stamp::now();
        Self::held().insert(space.id.clone(), space.clone());
    }

    fn remove(id: &str) {
        Self::held().remove(id);
    }

    fn holds(id: &str) -> bool {
        Self::held().contains_key(id)
    }

    fn prepare() -> bool {
        true
    }
}
