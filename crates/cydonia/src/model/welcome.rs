//! The project a first run opens on: two articles and a board, written into a
//! directory of cydonia's own.
//!
//! The content lives at `crates/cydonia/assets/welcome/.cydonia/` and is a
//! project in its own right — open that directory in the app to edit it. It is
//! embedded rather than fetched: a launch that cannot reach the network is
//! still a first launch, and the welcome describes the build it ships in.

use crate::model::{settings, state};
use crate::model::state::State;
use artifact::project::fs;
use std::{
    io::{self, Write as _},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// A directory under `assets/welcome/.cydonia`, and the file's bytes.
type File = (&'static str, &'static [u8]);

macro_rules! file {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/welcome/.cydonia/",
                $path
            )) as &[u8],
        )
    };
}

/// The entries, oldest first, each as the files it is made of.
///
/// Entries are listed newest first, so this is the reading order reversed:
/// `Start here`, then `Writing in Cydonia`, then `Getting started`. The order
/// is written into the mtimes by [`write`] rather than left to the filesystem
/// — every file of one seed is created inside the same millisecond, which is
/// the resolution the sort reads.
const SEED: &[&[File]] = &[
    &[file!("boards/1789958240578.toml")],
    &[
        file!("articles/1789958235652/content.md"),
        file!("articles/1789958235652/properties.toml"),
        file!("articles/1789958235652/cover-833563.svg"),
    ],
    &[
        file!("articles/1789958219063/content.md"),
        file!("articles/1789958219063/properties.toml"),
    ],
];

/// The entry a first run lands on, under `.cydonia/`. An article is remembered
/// by the path of its document rather than by its id — see
/// `workspace::articles`. It is the newest entry, so it is also the top of the
/// sidebar.
const LANDING: &str = "articles/1789958219063/content.md";

/// A second between entries: far coarser than the millisecond the sort reads,
/// and coarse enough that a filesystem storing whole seconds still orders them.
const APART: Duration = Duration::from_secs(1);

/// `~/.local/share/cydonia/welcome`, beside the agents and the grammars.
///
/// Cydonia's own directory rather than one of the user's: this is content the
/// app shipped, and deleting it costs nobody their work.
pub fn path() -> Option<PathBuf> {
    settings::data_dir().ok().map(|dir| dir.join("welcome"))
}

/// Put the welcome project on disk and name it in `state`, on the launch that
/// has no `state.toml` to restore.
///
/// That file is the whole record of having done this. It is written as soon as
/// the project list changes, so a welcome project the user deletes stays
/// deleted, and a machine that has ever opened cydonia is never seeded again.
///
/// Best effort: a seed that cannot be written leaves the front door standing,
/// which is what a launch with no projects shows anyway.
pub fn seed(state: &mut State) {
    if state::path().is_none_or(|path| path.exists()) {
        return;
    }
    let Some(root) = path() else {
        return;
    };
    // A directory already there is one whose `state.toml` went missing rather
    // than a first run. Its contents are the user's by now.
    if !root.is_dir() && write(&root).is_err() {
        return;
    }
    // Straight into the first article rather than the front door: a project
    // with something to read in it that opens on `Nothing open` reads as an
    // empty one.
    state.last.insert(
        root.clone(),
        state::Entry {
            kind: state::Kind::Article,
            id: fs::Project::new(&root)
                .cydonia()
                .join(LANDING)
                .to_string_lossy()
                .into_owned(),
        },
    );
    state.projects.insert(0, root);
    state.active = 0;
}

/// Write the seed under `root`, oldest entry first.
fn write(root: &Path) -> io::Result<()> {
    // The project's own `.cydonia/`, and the `*` ignore file with it — the
    // copy checked in beside the content ignores databases only, and shipping
    // that one would leave the seeded project visible to a repository the user
    // later makes here.
    let dir = fs::Project::new(root).init()?;
    let span = APART * SEED.len() as u32;
    let base = SystemTime::now()
        .checked_sub(span)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for (step, entry) in SEED.iter().enumerate() {
        let stamp = base + APART * step as u32;
        for (name, bytes) in *entry {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&path)?;
            file.write_all(bytes)?;
            file.set_modified(stamp)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/welcome.rs"]
mod tests;
