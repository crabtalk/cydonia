//! What a project's `.cydonia/` is doing while cydonia is not the one doing it.
//!
//! An agent runs with the project as its `cwd`, so the articles, boards and
//! tables on screen are things it can write. The project's backend knocks when
//! they change — see [`artifact::project::Project::watch`] — and the project
//! re-reads.
//!
//! The event is a knock, never the news. Which paths the OS reports and under
//! which kind is not something the platforms agree on — FSEvents coalesces a
//! burst into the directory that held it, an atomic save arrives as a rename
//! nobody paired, and a rename out of the tree looks like one into it. So a
//! knock carries nothing: the answer to any of them is the same re-read.

use crate::model::{store, workspace::Workspace};
use artifact::project::Project as _;
use bezel::gpui::{Context, Task};
use futures::{StreamExt as _, channel::mpsc};
use std::{path::PathBuf, time::Duration};

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
                let (tx, mut knocks) = mpsc::unbounded();
                let knock = move || {
                    let _ = tx.unbounded_send(());
                };
                let Some(watching) = store::open(&root).watch(knock) else {
                    // No watcher this backend will give us, and no event
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
                    // Armed on a stand-in: ask again, and keep whichever
                    // watch lands on the real thing.
                    if !watching.settled {
                        break;
                    }
                }
            }
        });
        Self { _pump: pump }
    }
}
