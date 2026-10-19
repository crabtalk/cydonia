//! The git backend a directory's repository is read through.
//!
//! [`open`] is the one place the gui picks a backend.

use std::{path::Path, sync::Arc};

pub use ::git::{Area, Backend, Change, Repository, Sources};

pub mod preview;

/// The backend for the repository at or above `dir`.
pub fn open(dir: &Path) -> Arc<dyn Backend> {
    let _ = dir;
    Arc::new(::git::Local)
}
