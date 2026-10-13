//! An article's properties: a toml file beside its content, holding what the
//! markdown does not — the title today, and the typed fields a page carries
//! next to it later.
//!
//! Edited in place with `toml_edit`, because an agent writing into this file is
//! expected: re-serialising it through a value tree would drop every key and
//! comment cydonia does not itself know about.

use crate::document;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const TITLE: &str = "title";

/// Put away, and dimmed under the divider — never a reason to drop the file.
const ARCHIVED: &str = "archived";

/// Whether the page is set across the pane rather than in the reading column.
/// Absent for a page that has never been told either way.
const FULL_WIDTH: &str = "full_width";

/// How the page's margin shows its comments. Absent for a page that has never
/// been told either way.
const COMMENTS: &str = "comments";

/// The article's labels — see [`crate::label`].
const LABELS: &str = "labels";

/// How an article's margin shows its comments.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentDisplay {
    /// Each comment beside its line.
    #[default]
    Full,
    /// A mark with a count beside each commented line; pressing it opens the
    /// comment.
    Minimal,
}

impl CommentDisplay {
    pub const ALL: [Self; 2] = [Self::Full, Self::Minimal];

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Minimal => "Minimal",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Minimal => "minimal",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|display| display.key() == key)
    }
}

/// Where this article's properties live — beside its content, in the directory
/// that is the article.
pub fn path(content: &Path) -> Option<PathBuf> {
    Some(document::properties(content))
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
    pub comments: Option<CommentDisplay>,
    pub labels: Vec<String>,
}

/// Read the whole file once and answer with all of it.
pub fn all(content: &Path) -> Properties {
    let Some(path) = path(content) else {
        return Properties::default();
    };
    parse(&std::fs::read_to_string(path).unwrap_or_default())
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

/// How the page's margin shows its comments, and `None` for a page nobody has
/// decided about — the same split as [`full_width`].
pub fn comments(content: &Path) -> Option<CommentDisplay> {
    all(content).comments
}

/// `None` takes the key out, handing the page back to the reader's default.
pub fn set_comments(content: &Path, display: Option<CommentDisplay>) {
    set(
        content,
        COMMENTS,
        display.map(|display| toml_edit::value(display.key())),
    );
}

/// Write every field at once, in one pass over the file. Keys this module does
/// not know about are kept.
pub fn save(content: &Path, properties: &Properties) -> std::io::Result<()> {
    document::save(content, &fields(properties))
}

/// Every field read out of a properties file's text.
pub fn parse(text: &str) -> Properties {
    let doc = document::parse(text);
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
        comments: doc
            .get(COMMENTS)
            .and_then(|display| display.as_str())
            .and_then(CommentDisplay::parse),
        labels: doc
            .get(LABELS)
            .and_then(|labels| labels.as_array())
            .map(|labels| {
                labels
                    .iter()
                    .filter_map(|label| label.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// A properties file's text with every field written in, keeping keys this
/// module does not know about. `None` when nothing is left to say, which is a
/// file that should not exist.
pub fn apply(text: &str, properties: &Properties) -> Option<String> {
    document::apply(text, &fields(properties))
}

fn fields(properties: &Properties) -> [(&'static str, Option<toml_edit::Item>); 5] {
    [
        (
            TITLE,
            (!properties.title.is_empty()).then(|| toml_edit::value(properties.title.as_str())),
        ),
        (
            ARCHIVED,
            properties.archived.then(|| toml_edit::value(true)),
        ),
        (FULL_WIDTH, properties.full_width.map(toml_edit::value)),
        (
            COMMENTS,
            properties
                .comments
                .map(|display| toml_edit::value(display.key())),
        ),
        (
            LABELS,
            (!properties.labels.is_empty()).then(|| {
                toml_edit::value(
                    properties
                        .labels
                        .iter()
                        .map(String::as_str)
                        .collect::<toml_edit::Array>(),
                )
            }),
        ),
    ]
}

/// Put a key in, or take it out when there is nothing to say. A properties file
/// with nothing left in it is removed: an article that has never been named
/// should not leave a file behind saying so.
fn set(content: &Path, key: &str, value: Option<toml_edit::Item>) {
    let _ = document::save(content, &[(key, value)]);
}
