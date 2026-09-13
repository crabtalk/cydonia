//! A project's articles: where they sit, and what is beside the document.
//!
//! An article is a directory — `articles/<id>/` holding `content.md`, the
//! [`properties`] beside it, and the cover. The markdown is content and
//! nothing else: an article is written to be read by the agent running in that
//! directory, and a path is how it gets handed over.
//!
//! The directory is named for when it was made, and nothing reads that name.
//! An id rather than a title: the title is a property, and a directory named
//! after it would be a second copy of it that a refused rename could leave
//! disagreeing — and a path already handed to an agent is not one we can
//! rewrite the way a vault rewrites its own links.
//!
//! The document a reader opens is the whole of an article that is here. What
//! it is opened *in* is the app's, and stays there.

pub mod properties;

use crate::{project::fs, stamp};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

/// An article as a reader gets one: what it is called, whether it is put away,
/// when it last changed, and where its picture is.
///
/// Not the document. The markdown is read when something opens it — the
/// sidebar lists every article in a project and opens none of them, so a body
/// on this struct would be every article's body loaded to draw a list.
///
/// The cover is a [`Url`] rather than a path because where it is, is the
/// backend's business: a file on this disk says `file://`, and one served over
/// a connection says so in the same field without the shape changing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// What names this article. The directory it is in, for the filesystem
    /// backend — the document moves within it and the name does not.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub archived: bool,
    /// When it last changed, as the same millisecond stamp ids carry — what
    /// the sidebar orders on.
    pub touched: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Url>,
}

/// Where a project's articles live, and what the document is called inside the
/// directory that is one.
const DIR: &str = "articles";
const CONTENT: &str = "content.md";

/// Where this project's articles are, whether or not any have been written.
pub fn dir(project: &Path) -> PathBuf {
    fs::Project::new(project).cydonia().join(DIR)
}

/// The same, made along with the `.cydonia/` it sits in.
pub fn init(project: &Path) -> std::io::Result<PathBuf> {
    Ok(fs::Project::new(project).init()?.join(DIR))
}

/// The document inside one article's directory — the path an agent is given.
pub fn content(article: &Path) -> PathBuf {
    article.join(CONTENT)
}

/// When the article was last written, whichever of its files took the write.
///
/// Both of them, because naming a page *is* writing it and the name lives in
/// the properties: ordered on the markdown alone, an article renamed and never
/// otherwise touched sinks back down the list the moment it is re-read.
///
/// The properties are only asked about when they are there — [`stamp::of`]
/// answers `now` for a file it cannot stat, which would float every article
/// that has never had a property to the top and keep it moving.
pub fn touched(content: &Path) -> u128 {
    let written = stamp::of(content);
    match properties::path(content).filter(|path| path.is_file()) {
        Some(properties) => written.max(stamp::of(&properties)),
        None => written,
    }
}

/// This millisecond's directory, or the first after it that is not taken. Two
/// articles made inside one millisecond is the only way that happens.
/// What names the article a document sits in: the directory it is in, which
/// is what a reader asks for it by.
pub fn id_of(content: &Path) -> String {
    content
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map_or_else(crate::id::mint, str::to_owned)
}

pub fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(stamp.to_string()))
        .find(|article| !article.exists())
        .unwrap_or_else(|| dir.join(stamp.to_string()))
}
