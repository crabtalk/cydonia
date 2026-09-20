//! An article's properties: a toml file beside its content, holding what the
//! markdown does not — the title today, and the typed fields a page carries
//! next to it later.
//!
//! Edited in place with `toml_edit`, because an agent writing into this file is
//! expected: re-serialising it through a value tree would drop every key and
//! comment cydonia does not itself know about.

use std::path::{Path, PathBuf};

/// What it is called inside the article's own directory.
const FILE: &str = "properties.toml";

const TITLE: &str = "title";

/// Put away, and dimmed under the divider — never a reason to drop the file.
const ARCHIVED: &str = "archived";

/// Whether the page is set across the pane rather than in the reading column.
/// Absent for a page that has never been told either way.
const FULL_WIDTH: &str = "full_width";

/// Where this article's properties live — beside its content, in the directory
/// that is the article.
pub fn path(content: &Path) -> Option<PathBuf> {
    Some(content.with_file_name(FILE))
}

/// Everything a listing reads off one article, in one pass over the file.
///
/// The fields have accessors of their own and every one of them opens and
/// parses `properties.toml`, so a caller that wants two of them pays twice —
/// which is what listing a project used to cost, three reads an article. A
/// caller after a single field still reaches for the accessor.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Properties {
    pub title: String,
    pub archived: bool,
    pub full_width: Option<bool>,
}

/// Read the whole file once and answer with all of it.
pub fn all(content: &Path) -> Properties {
    let Some(path) = path(content) else {
        return Properties::default();
    };
    let doc = read(&path);
    Properties {
        title: doc
            .get(TITLE)
            .and_then(|title| title.as_str())
            .unwrap_or_default()
            .to_owned(),
        archived: doc
            .get(ARCHIVED)
            .and_then(|archived| archived.as_bool())
            .unwrap_or_default(),
        full_width: doc.get(FULL_WIDTH).and_then(|wide| wide.as_bool()),
    }
}

/// The article's title, or nothing for one that has never been given a name.
pub fn title(content: &Path) -> String {
    all(content).title
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
    all(content).archived
}

pub fn set_archived(content: &Path, archived: bool) {
    set(content, ARCHIVED, archived.then(|| toml_edit::value(true)));
}

/// Whether the page is set across the pane, and `None` for a page nobody has
/// decided about.
///
/// A property of the document and not of this machine, when it is written at
/// all: a page of wide tables is wide for whoever opens the project. Unset is
/// the other half of that — a page with nothing of its own to say is the
/// reader's own default to answer, and the app is where that default is kept.
pub fn full_width(content: &Path) -> Option<bool> {
    all(content).full_width
}

/// `None` takes the key out, handing the page back to the reader's default.
pub fn set_full_width(content: &Path, wide: Option<bool>) {
    set(content, FULL_WIDTH, wide.map(toml_edit::value));
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
