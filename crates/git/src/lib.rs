//! The git requests cydonia asks of a repository, answered by a [`Backend`]:
//! [`Local`] on this machine, [`Remote`] through the [`Git`] Cupola service
//! on another.
//!
//! Requests and answers are serde types. Paths cross the wire as bytes, so a
//! name that is not UTF-8 arrives as it left.

use anyhow::Result;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod local;
mod remote;
pub mod service;

pub use local::Local;
pub use remote::Remote;
pub use service::Git;

/// Answers git requests about the repository a directory is in. A call
/// returns at once; its future resolves when git has answered.
pub trait Backend: Send + Sync {
    /// The repository at or above `dir` and its changes, or `None` when `dir`
    /// is in no repository.
    fn status(&self, dir: &Path) -> BoxFuture<'static, Result<Option<Repository>>>;

    /// The patch of `change` in the repository at `root`. A patch over 1 MiB
    /// is cut there and says so; an empty one is a sentence saying there is no
    /// textual diff.
    fn diff(&self, root: &Path, change: &Change) -> BoxFuture<'static, Result<String>>;

    /// The file's text on each side of `change`. A side is `None` when it does
    /// not exist, is over 1 MiB, is not UTF-8 text, or could not be read.
    fn sources(&self, root: &Path, change: &Change) -> BoxFuture<'static, Sources>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Area {
    Staged,
    Unstaged,
    Untracked,
}

impl Area {
    pub fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged",
            Self::Unstaged => "Unstaged",
            Self::Untracked => "Untracked",
        }
    }
}

/// One changed file in one [`Area`]. A conflict is one change, in
/// [`Area::Unstaged`] with status `U`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    /// Relative to the repository's root.
    #[serde(with = "cupola::bytes::path")]
    pub path: PathBuf,
    /// The source of a rename or copy, relative to the repository's root.
    #[serde(with = "cupola::bytes::optional_path")]
    pub original: Option<PathBuf>,
    pub area: Area,
    /// Git's porcelain status letter.
    pub status: char,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// The working tree's top level, as git names it.
    #[serde(with = "cupola::bytes::path")]
    pub root: PathBuf,
    /// Staged, then unstaged, then untracked; by path within each.
    pub files: Vec<Change>,
}

/// The text of a changed file before and after its change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sources {
    pub old: Option<String>,
    pub new: Option<String>,
}
