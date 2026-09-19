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

use crate::{entry, project::fs, stamp};
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

// ── moving one to another project ────────────────────────────────

/// Carry an article to another project, document, cover, properties and all.
///
/// Takes the path of the `content.md`, and answers the path it has afterwards.
/// The article keeps its id where the destination has that name free, and takes
/// the next free one where it does not.
///
/// The pictures in the body are copied into the destination's `assets/` and the
/// document rewritten to point at them — see [`carry_assets`]. The cover sits in
/// the article's own directory and needs none of that.
///
/// The `#number` does not come along: the source tombstones its own, and the
/// destination issues one on the next read.
pub fn move_to(content: &Path, to: &Path) -> std::io::Result<PathBuf> {
    let missing =
        |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, what.to_owned());
    let from_dir = content.parent().ok_or_else(|| missing("no article here"))?;
    let from = project_of(content).ok_or_else(|| missing("no project here"))?;
    if from == to {
        return Ok(content.to_owned());
    }
    let id = id_of(content);
    let dir = init(to)?;
    let landing = match dir.join(&id) {
        taken if taken.exists() => free(&dir, crate::stamp::now()),
        free => free,
    };
    carry(from_dir, &landing)?;
    let arrived = self::content(&landing);
    carry_assets(&arrived, from, to);
    let _ = entry::Registry::open(from).and_then(|registry| registry.remove("article", &id));
    Ok(arrived)
}

/// The project a `content.md` is in: `<project>/.cydonia/articles/<id>/content.md`.
pub fn project_of(content: &Path) -> Option<&Path> {
    content.ancestors().nth(4)
}

/// Move the directory, falling back to a copy where a rename cannot cross what
/// is between the two — a project on another disk is the usual reason.
fn carry(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        match entry.file_type()?.is_dir() {
            true => copy_dir(&source, &target)?,
            false => {
                std::fs::copy(&source, &target)?;
            }
        }
    }
    Ok(())
}

/// Bring the pictures the document points at along with it.
///
/// A body holds whole paths into the project's `assets/`, which is why a move
/// has to touch the document at all. The file names are hashes of the bytes, so
/// a picture already in the destination is the same file and is not copied
/// again.
///
/// Best effort: the move stands whether or not the pictures followed.
fn carry_assets(content: &Path, from: &Path, to: &Path) {
    let Ok(text) = std::fs::read_to_string(content) else {
        return;
    };
    let (here, there) = (
        fs::Project::new(from).assets(),
        fs::Project::new(to).assets(),
    );
    let (here, there) = (here.to_string_lossy(), there.to_string_lossy());
    if !text.contains(here.as_ref()) {
        return;
    }
    for name in assets_named(&text, &here) {
        let (source, target) = (
            Path::new(here.as_ref()).join(&name),
            Path::new(there.as_ref()).join(&name),
        );
        if target.exists() {
            continue;
        }
        if std::fs::create_dir_all(there.as_ref()).is_ok() {
            let _ = std::fs::copy(&source, &target);
        }
    }
    let _ = std::fs::write(content, text.replace(here.as_ref(), there.as_ref()));
}

/// The file names under `assets` the document mentions. Everything up to what
/// cannot be in one: a path in markdown is followed by a quote, a bracket or
/// the end of the line, and none of those are in a name this app writes.
fn assets_named(text: &str, assets: &str) -> Vec<String> {
    let mut names = Vec::new();
    for rest in text.split(assets).skip(1) {
        let rest = rest.strip_prefix(std::path::MAIN_SEPARATOR).unwrap_or(rest);
        let name: String = rest
            .chars()
            .take_while(|c| !matches!(c, '"' | '\'' | ')' | ']' | '>' | '\n' | '\r' | ' '))
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}
