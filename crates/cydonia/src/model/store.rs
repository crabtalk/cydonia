//! The backend a project's work is kept in, one per project path.
//!
//! Every read and write of boards, sessions, articles and numbers goes through
//! the handle [`open`] answers, and `open` is the one place that picks the
//! backend.

use artifact::project::{Project, fs};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub type Store = Arc<dyn Project + Send + Sync>;

/// The same handle for every caller asking after one path, for the life of the
/// process.
pub fn open(path: &Path) -> Store {
    static OPEN: OnceLock<Mutex<HashMap<PathBuf, Store>>> = OnceLock::new();
    let mut open = OPEN
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    open.entry(path.to_owned())
        .or_insert_with(|| Arc::new(fs::Project::new(path)))
        .clone()
}
