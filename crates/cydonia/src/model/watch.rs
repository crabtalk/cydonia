//! What a project's `.cydonia/` is doing while cydonia is not the one doing it.
//!
//! An agent runs with the project as its `cwd`, so the articles, boards and
//! tables on screen are files it can write. The watch is what makes that show
//! up: the OS says the directory moved, and the project re-reads it.
//!
//! The event is a knock, never the news. Which paths a backend reports and
//! under which kind is not something the platforms agree on — FSEvents
//! coalesces a burst into the directory that held it, an atomic save arrives as
//! a rename nobody paired, and a rename out of the tree looks like one into it.
//! So nothing here reads an event beyond *where* it landed: the answer to any
//! of them is the same re-read.

use crate::{data, model::workspace::Workspace};
use bezel::gpui::{Context, Task};
use futures::{StreamExt as _, channel::mpsc};
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use schema::project;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// How quiet the directory has to go before it is re-read, in milliseconds, and
/// what the setting behind it defaults to. Writing one document is a string of
/// events — the content, the properties beside it, the directory holding both —
/// and re-reading on each would be re-reading a file mid-write.
pub const BOUNCE: u64 = 150;

/// What the setting may be wound to. `settings.toml` is meant to be edited by
/// hand, so both ends are enforced on the way out of it rather than trusted:
/// a bounce of nothing is a re-read per event, which for the store is a re-read
/// of our own re-read.
pub const BOUNCE_RANGE: (u64, u64) = (20, 2_000);

/// The bounce as the timer wants it, clamped — see [`BOUNCE_RANGE`].
pub fn bounce(ms: u64) -> Duration {
    Duration::from_millis(ms.clamp(BOUNCE_RANGE.0, BOUNCE_RANGE.1))
}

/// A live watch on one project. Dropping it is what takes the watch down, so a
/// closed project unwatches itself and nothing has to remember to.
pub struct Watch {
    /// Held rather than detached: the watcher lives inside the task, and
    /// dropping the task is what drops it.
    _pump: Task<()>,
}

impl Watch {
    /// Watch a project's `.cydonia/`, and reconcile the project whenever it
    /// moves.
    ///
    /// The project is found again by path on each pass rather than held by
    /// index: the rail can be reordered and projects closed while this runs,
    /// and the index it was armed on would by then be somebody else's.
    pub fn open(root: PathBuf, cx: &mut Context<Workspace>) -> Self {
        let pump = cx.spawn(async move |workspace, cx| {
            loop {
                let Some((_watcher, mut knocks, deep)) = arm(&root) else {
                    // No watcher this platform will give us, and no event
                    // coming to say otherwise. Watching nothing beats spinning.
                    return;
                };
                loop {
                    if knocks.next().await.is_none() {
                        return;
                    }
                    // Read per pass rather than captured: moving the setting
                    // takes effect on the next event, with nothing to re-arm.
                    let settle = workspace
                        .read_with(cx, |workspace, _| bounce(workspace.settings.watch_bounce))
                        .unwrap_or_else(|_| bounce(BOUNCE));
                    cx.background_executor().timer(settle).await;
                    while knocks.try_recv().is_ok() {}
                    let held = workspace
                        .update(cx, |workspace, cx| workspace.reload_project(&root, cx))
                        .is_ok();
                    // The workspace went away, and the watch goes with it.
                    if !held {
                        return;
                    }
                    // Armed on the project because it had no `.cydonia/` yet,
                    // and now it has one: drop out and arm on the real thing.
                    if !deep && project::dir(&root).exists() {
                        break;
                    }
                }
            }
        });
        Self { _pump: pump }
    }
}

/// Put a watcher up, and say whether it landed on what was actually wanted.
///
/// `.cydonia/` is made the first time a project keeps anything, which can be
/// long after it was opened — so a project without one is watched shallowly at
/// its own root, where the one event that matters is the directory appearing.
fn arm(root: &Path) -> Option<(RecommendedWatcher, mpsc::UnboundedReceiver<()>, bool)> {
    // The prefix every event is matched against, resolved once. FSEvents
    // reports the real path, so a project reached through a symlink would never
    // match the prefix it was armed with.
    let dir = project::dir(&std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned()));
    let (tx, rx) = mpsc::unbounded();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if event.paths.iter().any(|path| ours(&dir, path)) {
            let _ = tx.unbounded_send(());
        }
    })
    .ok()?;
    match watcher.watch(&project::dir(root), RecursiveMode::Recursive) {
        Ok(()) => Some((watcher, rx, true)),
        Err(_) => watcher
            .watch(root, RecursiveMode::NonRecursive)
            .ok()
            .map(|()| (watcher, rx, false)),
    }
}

/// Whether a path that moved is one this app reads back.
///
/// Sessions are left out on purpose. This process writes a transcript on every
/// frame of a streaming turn, so a watch that covered them would be a watch on
/// ourselves — and a session is a live connection, not something that could be
/// adopted back off disk anyway.
fn ours(dir: &Path, path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(dir) else {
        // Outside `.cydonia/`, where the only thing worth a knock is the
        // directory itself coming into existence.
        return path == dir;
    };
    let Some(head) = rest.components().next() else {
        return true;
    };
    let head = head.as_os_str().to_string_lossy();
    if head == "articles" || head == "boards" {
        return true;
    }
    // The store, and the log a commit actually lands in — the database runs in
    // WAL, so the file itself only moves at a checkpoint.
    //
    // Named one by one rather than taken by prefix, to leave `-shm` out. That
    // is the reader's shared index, and *this* process writes it every time it
    // reads a table back — a watch on it would be a watch on our own re-reads,
    // and the loop between the two would never settle.
    let Some(tail) = head.strip_prefix(data::FILE) else {
        return false;
    };
    matches!(tail, "" | "-wal" | "-journal")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The filter, and the two paths that must not pass it. Written down
    /// because both failures are silent: a watch that lets `-shm` through spins
    /// forever against its own reads, and one that lets `sessions/` through
    /// spins against its own transcripts.
    #[test]
    fn only_what_is_read_back_knocks() {
        let dir = Path::new("/p/.cydonia");
        for path in [
            "/p/.cydonia",
            "/p/.cydonia/articles/1/content.md",
            "/p/.cydonia/boards/1.toml",
            "/p/.cydonia/data.db",
            "/p/.cydonia/data.db-wal",
        ] {
            assert!(ours(dir, Path::new(path)), "{path} should knock");
        }
        for path in [
            "/p/.cydonia/data.db-shm",
            "/p/.cydonia/sessions/1.json",
            "/p/src/main.rs",
            "/other/.cydonia/articles/1/content.md",
        ] {
            assert!(!ours(dir, Path::new(path)), "{path} should not knock");
        }
    }
}
