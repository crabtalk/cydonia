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

pub mod anchor;
pub mod cover;
pub mod properties;

#[cfg(feature = "sqlite")]
use crate::entry;
use crate::project::fs;
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
pub(crate) const DIR: &str = "articles";

pub use crate::document::{assets, content, touched};
use crate::document::{carry, carry_assets, repoint};

/// Where this project's articles are, whether or not any have been written.
pub fn dir(project: &Path) -> PathBuf {
    fs::Project::new(project).cydonia().join(DIR)
}

/// The same, made along with the `.cydonia/` it sits in.
pub fn init(project: &Path) -> std::io::Result<PathBuf> {
    Ok(fs::Project::new(project).init()?.join(DIR))
}

/// What names the article a document sits in: the directory it is in, which
/// is what a reader asks for it by.
pub fn id_of(content: &Path) -> String {
    content
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map_or_else(crate::id::mint, str::to_owned)
}

/// This millisecond's directory, or the first after it that is not taken. Two
/// articles made inside one millisecond is the only way that happens.
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
/// The article's own `assets/` travels with the directory, and the whole paths
/// the body holds into it are rewritten — see [`repoint`]. A body written
/// before those existed points into the source project's shared `assets/`, and
/// those pictures are copied and the document rewritten — see [`carry_assets`].
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
    repoint(&arrived, from_dir, &landing);
    carry_assets(&arrived, from, to);
    #[cfg(feature = "sqlite")]
    let _ = entry::Registry::open(from).and_then(|registry| registry.remove("article", &id));
    Ok(arrived)
}

/// Take an article off the disk: the directory is the article, cover and
/// properties included.
///
/// The number it was issued goes with it — a reference that outlived the thing
/// it named would be read back as an article nobody can open. Best effort on
/// the registry alone: the files are gone either way, and a number left behind
/// is retired by the next read.
pub fn remove(content: &Path) -> std::io::Result<()> {
    let dir = content
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no article here"))?;
    let id = id_of(content);
    let project = project_of(content).map(Path::to_path_buf);
    std::fs::remove_dir_all(dir)?;
    #[cfg(feature = "sqlite")]
    if let Some(project) = project {
        let _ =
            entry::Registry::open(&project).and_then(|registry| registry.remove("article", &id));
    }
    #[cfg(not(feature = "sqlite"))]
    let _ = (id, project);
    Ok(())
}

/// The project a `content.md` is in: `<project>/.cydonia/articles/<id>/content.md`.
pub fn project_of(content: &Path) -> Option<&Path> {
    content.ancestors().nth(4)
}
