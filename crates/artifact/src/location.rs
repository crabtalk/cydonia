//! Where a project is.
//!
//! Written to a file as the bare path a [`Location::Local`] holds, the shape
//! `state.toml` and spaces held before this type.

use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Location {
    /// A directory on this machine.
    Local(PathBuf),
}

impl Location {
    /// The project's directory, on whichever machine holds it.
    pub fn root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
        }
    }

    /// The project's directory, where that is on this machine.
    pub fn local(&self) -> Option<&Path> {
        match self {
            Self::Local(root) => Some(root),
        }
    }
}

impl From<PathBuf> for Location {
    fn from(root: PathBuf) -> Self {
        Self::Local(root)
    }
}

impl From<&Path> for Location {
    fn from(root: &Path) -> Self {
        Self::Local(root.to_path_buf())
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(root) => root.display().fmt(f),
        }
    }
}
