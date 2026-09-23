//! The projects the app has open, from a tool's side of the glass.
//!
//! Everything else here is a directory and a file layout, which is why the rest
//! of this crate holds nothing at all: a project is wherever the call says it
//! is, opened or not. Which projects a *window* is showing is not like that —
//! it is the app's own list, in memory, rewritten whole every time it moves —
//! so this is the one place the two have to meet.
//!
//! The app installs what to do about a change and pushes what it is holding; a
//! tool asks, and reads. **Asked rather than done**: a call arrives on whichever
//! thread the door is serving from, and the rail can only be moved on the one
//! the window is drawn on. So a tool hands its change over and answers for the
//! part it did itself — the directory — which is the part that can fail.
//!
//! Installed once at launch, like a highlighter or a link preview. A build with
//! nothing installed — the tests, a caller that mounted the tool sets on their
//! own — refuses rather than pretends.

use crate::tool::Trouble;
use std::{
    path::{Path, PathBuf},
    sync::RwLock,
};

/// What a tool asks of the rail. Closing takes a project off it and stops the
/// agents running in it; nothing on disk is touched either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Open(PathBuf),
    Close(PathBuf),
    /// A prompt for the session filed under `session` — its record id, which
    /// is unique across projects.
    Send { session: String, message: String },
    /// A new session in `project` on the agent `settings.toml` names `agent`,
    /// with `message` as its first prompt.
    Start {
        project: PathBuf,
        agent: String,
        message: String,
    },
}

type Hand = Box<dyn Fn(Change) + Send + Sync>;

/// What the app does about a change.
static HAND: RwLock<Option<Hand>> = RwLock::new(None);

/// What the app is holding, as it last left it.
static OPEN: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Hand the rail over. The app calls this once, at launch.
pub fn install(hand: impl Fn(Change) + Send + Sync + 'static) {
    if let Ok(mut held) = HAND.write() {
        *held = Some(Box::new(hand));
    }
}

/// Say what is on the rail. Pushed by the app wherever the list is written
/// down, rather than read back off a file: `state.toml` is the app's to rewrite
/// whole, and a tool reading it would be racing the next save.
pub fn set_open(projects: Vec<PathBuf>) {
    if let Ok(mut held) = OPEN.write() {
        *held = projects;
    }
}

/// The projects on the rail, in the order the app lists them.
pub fn open() -> Vec<PathBuf> {
    OPEN.read().map(|held| held.clone()).unwrap_or_default()
}

/// Whether the rail is holding a project at `path`.
///
/// Either spelling of it counts. The sidebar holds whatever the directory
/// picker was given and the tools settle a path before asking about one, so
/// `/tmp/x` and `/private/tmp/x` reach here as one project — the app resolves
/// the same pair on its side, in `Workspace::project_at`.
pub fn is_open(path: &Path) -> bool {
    let settled = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let want = settled(path);
    open()
        .iter()
        .any(|held| held == path || settled(held) == want)
}

/// Ask for the change, which is as far as a tool can take it.
pub(crate) fn ask(change: Change) -> Result<(), Trouble> {
    let held = HAND.read().map_err(|_| nobody())?;
    let Some(hand) = held.as_ref() else {
        return Err(nobody());
    };
    hand(change);
    Ok(())
}

fn nobody() -> Trouble {
    Trouble::Refused("no cydonia window to open a project in".to_owned())
}
