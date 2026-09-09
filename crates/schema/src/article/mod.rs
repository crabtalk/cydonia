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

use crate::project;
use std::path::{Path, PathBuf};

/// Where a project's articles live, and what the document is called inside the
/// directory that is one.
const DIR: &str = "articles";
const CONTENT: &str = "content.md";

/// Where this project's articles are, whether or not any have been written.
pub fn dir(project: &Path) -> PathBuf {
    project::dir(project).join(DIR)
}

/// The same, made along with the `.cydonia/` it sits in.
pub fn init(project: &Path) -> std::io::Result<PathBuf> {
    Ok(project::init(project)?.join(DIR))
}

/// The document inside one article's directory — the path an agent is given.
pub fn content(article: &Path) -> PathBuf {
    article.join(CONTENT)
}

/// This millisecond's directory, or the first after it that is not taken. Two
/// articles made inside one millisecond is the only way that happens.
pub fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(stamp.to_string()))
        .find(|article| !article.exists())
        .unwrap_or_else(|| dir.join(stamp.to_string()))
}
