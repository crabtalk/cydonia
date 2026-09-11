//! Where a project keeps what cydonia writes, and the stamp everything in it
//! is named by.
//!
//! The layout, not the project — [`crate`] holds the shapes a project's
//! `.cydonia/` is made of, and this is the one thing they all share: which
//! directory they sit in. The app's own `Project`, with the sessions running
//! in it and the panes over them, is a different thing and stays there.
//!
//! Every path here is derived from the project directory rather than stored,
//! because the project directory is the whole identity — it is what a session
//! is spawned with as its `cwd`, and moving it moves everything under it.

use std::path::{Path, PathBuf};

/// Everything cydonia holds for a project lives here: its articles, its
/// archived sessions, and its database.
const DIR: &str = ".cydonia";

pub fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

/// The same directory, made if it is not there, and carrying the `.gitignore`
/// that keeps the whole of it out of the repo it sits in — none of what cydonia
/// writes here is the project's source.
///
/// Every path that creates the directory comes through here. A second
/// `create_dir_all` elsewhere would make it without the ignore file, and
/// whichever ran first would decide whether the repo sees a database.
pub fn init(project: &Path) -> std::io::Result<PathBuf> {
    let dir = dir(project);
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    Ok(dir)
}
