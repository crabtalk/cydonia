//! An article's properties: a toml file beside its content, holding what the
//! markdown does not — the title today, and the typed fields a page carries
//! next to it later.
//!
//! Edited in place with `toml_edit` like [`crate::model::settings`], because an
//! agent writing into this file is expected: re-serialising it through a value
//! tree would drop every key and comment cydonia does not itself know about.

use std::path::{Path, PathBuf};

/// What it is called inside the article's own directory.
const FILE: &str = "properties.toml";

const TITLE: &str = "title";

/// Put away, and dimmed under the divider — never a reason to drop the file.
const ARCHIVED: &str = "archived";

/// Where this article's properties live — beside its content, in the directory
/// that is the article.
pub fn path(content: &Path) -> Option<PathBuf> {
    Some(content.with_file_name(FILE))
}

/// The article's title, or nothing for one that has never been given a name.
pub fn title(content: &Path) -> String {
    let Some(path) = path(content) else {
        return String::new();
    };
    read(&path)
        .get(TITLE)
        .and_then(|title| title.as_str())
        .unwrap_or_default()
        .to_owned()
}

pub fn set_title(content: &Path, title: &str) {
    set(
        content,
        TITLE,
        (!title.is_empty()).then(|| toml_edit::value(title)),
    );
}

/// Whether the article has been put away.
pub fn archived(content: &Path) -> bool {
    let Some(path) = path(content) else {
        return false;
    };
    read(&path)
        .get(ARCHIVED)
        .and_then(|archived| archived.as_bool())
        .unwrap_or_default()
}

pub fn set_archived(content: &Path, archived: bool) {
    set(content, ARCHIVED, archived.then(|| toml_edit::value(true)));
}

/// Put a key in, or take it out when there is nothing to say. A properties file
/// with nothing left in it is removed: an article that has never been named
/// should not leave a file behind saying so.
fn set(content: &Path, key: &str, value: Option<toml_edit::Item>) {
    let Some(path) = path(content) else {
        return;
    };
    let mut doc = read(&path);
    match value {
        Some(value) => doc[key] = value,
        None => {
            doc.remove(key);
        }
    }
    if doc.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let _ = std::fs::write(&path, doc.to_string());
}

fn read(path: &Path) -> toml_edit::DocumentMut {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or_default()
}
