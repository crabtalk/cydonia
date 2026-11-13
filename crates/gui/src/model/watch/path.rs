use crate::model::{
    disk::{Files as _, Here},
    watch,
    workspace::Workspace,
};
use bezel::gpui::{App, AppContext as _, Context, Entity, Global, Task, WeakEntity};
use futures::{StreamExt as _, channel::mpsc};
use std::{collections::HashMap, path::PathBuf, time::SystemTime};

#[derive(Default)]
struct OpenPaths(HashMap<PathBuf, WeakEntity<Changes>>);
impl Global for OpenPaths {}

pub type Stamp = (SystemTime, u64);

pub struct Changes {
    pub stamp: Option<Stamp>,
    pub ready: bool,
    pub path: PathBuf,
    _pump: Task<()>,
}

impl Changes {
    pub fn open(path: PathBuf, workspace: WeakEntity<Workspace>, cx: &mut App) -> Entity<Self> {
        let path = std::path::absolute(&path).unwrap_or(path);
        let paths = &mut cx.default_global::<OpenPaths>().0;
        paths.retain(|_, changes| changes.is_upgradable());
        if let Some(changes) = paths.get(&path).and_then(WeakEntity::upgrade) {
            return changes;
        }
        let key = path.clone();
        let changes = cx.new(|cx: &mut Context<Self>| {
            let watched_path = path.clone();
            let pump = cx.spawn(async move |this, cx| {
                loop {
                    let (tx, mut knocks) = mpsc::unbounded();
                    let watch_path = path.clone();
                    let watching = cx
                        .background_executor()
                        .spawn(async move {
                            Here::watch(&watch_path, move || {
                                let _ = tx.unbounded_send(());
                            })
                        })
                        .await;
                    let read_path = path.clone();
                    let stamp = cx
                        .background_executor()
                        .spawn(async move {
                            let meta = std::fs::metadata(read_path).ok()?;
                            Some((meta.modified().ok()?, meta.len()))
                        })
                        .await;
                    if this
                        .update(cx, |this, cx| {
                            this.stamp = stamp;
                            this.ready = true;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                    let Some(_watching) = watching else { return };
                    if knocks.next().await.is_none() {
                        return;
                    }
                    let delay = workspace
                        .read_with(cx, |workspace, _| {
                            watch::bounce(workspace.settings.watch_bounce)
                        })
                        .unwrap_or_else(|_| watch::bounce(watch::BOUNCE));
                    cx.background_executor().timer(delay).await;
                    while knocks.try_recv().is_ok() {}
                }
            });
            Self {
                stamp: None,
                ready: false,
                path: watched_path,
                _pump: pump,
            }
        });
        cx.default_global::<OpenPaths>()
            .0
            .insert(key, changes.downgrade());
        changes
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/path_changes.rs"]
pub(crate) mod tests;
