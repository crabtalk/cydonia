//! The skills the configured agents read from disk. Held by the workspace,
//! scanned when asked for rather than watched.

use bezel::gpui::{AppContext as _, Context, SharedString};
use skills::{Harness, Skill};
use std::path::PathBuf;

#[derive(Default)]
pub struct Skills {
    /// As the last scan found them, by agent and then by name.
    pub list: Vec<Skill>,
    /// Why the last switch was not written, until the next one is.
    pub error: Option<SharedString>,
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

    /// Switch `skill` on or off in its agent's settings, off the UI thread,
    /// and list it so once that is written.
    pub fn set_enabled(&mut self, skill: Skill, on: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn({
                    let skill = skill.clone();
                    async move { skill.set_enabled(on) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match written {
                    Ok(()) => {
                        this.error = None;
                        for listed in &mut this.list {
                            if listed.harness == skill.harness && listed.path == skill.path {
                                listed.enabled = on;
                            }
                        }
                    }
                    Err(err) => this.error = Some(format!("{}: {err:#}", skill.name).into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
