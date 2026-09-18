//! Where layouts are kept: `~/.config/cydonia/layouts/`, beside the state file
//! rather than inside any project.
//!
//! A layout spans projects — a pane on a session in one repo beside a pane on
//! a board in another — so it belongs to no single `.cydonia/`. What it holds
//! is absolute paths on this machine, which is the same reason
//! [`crate::model::state`] lives here: neither is worth carrying to another
//! machine, and neither is the project's source.
//!
//! One file per layout, named for the millisecond it was made, the way boards
//! are inside a project. Best effort throughout: a layout that cannot be
//! written is not worth failing a drag over.

use crate::model::settings;
use artifact::{layout::Layout, stamp};
use std::{collections::HashSet, path::PathBuf};

/// `~/.config/cydonia/layouts/`.
fn dir() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("layouts"))
}

fn file(id: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("{id}.toml")))
}

/// Every layout on this machine, most recently written first.
pub fn all() -> Vec<Layout> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut layouts: Vec<Layout> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| {
            let body = std::fs::read_to_string(&path).ok()?;
            let mut layout: Layout = toml::from_str(&body).ok()?;
            layout.touched = stamp::of(&path);
            if layout.id.is_empty() {
                layout.id = path.file_stem()?.to_str()?.to_owned();
            }
            Some(layout)
        })
        .collect();
    layouts.sort_by_key(|layout| std::cmp::Reverse(layout.touched));
    layouts
}

/// Read one back, for a test or a re-read.
pub fn read(id: &str) -> Option<Layout> {
    let path = file(id)?;
    let body = std::fs::read_to_string(&path).ok()?;
    let mut layout: Layout = toml::from_str(&body).ok()?;
    layout.touched = stamp::of(&path);
    if layout.id.is_empty() {
        layout.id = id.to_owned();
    }
    Some(layout)
}

/// Mint one, named clear of those already here.
pub fn create(name: &str, over: artifact::layout::Member) -> Option<Layout> {
    let dir = dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let taken: HashSet<String> = all().into_iter().map(|layout| layout.name).collect();
    let name = match name.is_empty() {
        false => name.to_owned(),
        true => artifact::layout::next_name(&taken),
    };
    // This millisecond's file, or the first after it that is not taken — two
    // made inside one millisecond is the only way that happens.
    let id = (stamp::now()..)
        .map(|stamp| stamp.to_string())
        .find(|id| !dir.join(format!("{id}.toml")).exists())?;
    let mut layout = Layout::new(id, &name, over);
    save(&mut layout);
    Some(layout)
}

/// Write one back, and take the time it was written at — what the sidebar
/// orders on.
pub fn save(layout: &mut Layout) {
    let (Some(path), Ok(body)) = (file(&layout.id), toml::to_string_pretty(&*layout)) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(&path, body).is_ok() {
        layout.touched = stamp::now();
    }
}

pub fn remove(id: &str) {
    if let Some(path) = file(id) {
        let _ = std::fs::remove_file(path);
    }
}
