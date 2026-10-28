//! The skills the configured agents read from disk. Held by the workspace,
//! scanned when asked for rather than watched.

use bezel::gpui::{AppContext as _, Context};
use skills::{Harness, Skill};
use std::path::PathBuf;

#[derive(Default)]
pub struct Skills {
    /// As the last scan found them, by agent and then by name.
    pub list: Vec<Skill>,
}

impl Skills {
    /// Scan the roots of each of `harnesses` over `projects`, off the UI
    /// thread, and list what is found in place of what was.
    pub fn scan(
        &mut self,
        harnesses: Vec<Harness>,
        projects: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move {
                    harnesses
                        .into_iter()
                        .flat_map(|harness| harness.scan(&projects))
                        .collect()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.list = list;
                cx.notify();
            });
        })
        .detach();
    }
}
