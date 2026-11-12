use crate::model::{
    disk::{Files as _, Here},
    watch,
    workspace::Workspace,
};
use bezel::gpui::{App, AppContext as _, Context, Entity, Global, Task, WeakEntity};
use futures::{StreamExt as _, channel::mpsc};
use std::{collections::HashMap, path::PathBuf};

#[derive(Default)]
struct OpenFiles(HashMap<PathBuf, WeakEntity<File>>);
impl Global for OpenFiles {}

pub struct File {
    pub result: Option<Result<String, String>>,
    revision: u64,
    _pump: Task<()>,
}

impl File {
    pub fn open(path: PathBuf, workspace: WeakEntity<Workspace>, cx: &mut App) -> Entity<Self> {
        let path = Here::resolve(&path);
        let files = &mut cx.default_global::<OpenFiles>().0;
        files.retain(|_, file| file.is_upgradable());
        if let Some(file) = files.get(&path).and_then(WeakEntity::upgrade) {
            return file;
        }
        let key = path.clone();
        let file = cx.new(|cx: &mut Context<Self>| {
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
                    let Ok(revision) = this.read_with(cx, |this, _| this.revision) else {
                        return;
                    };
                    let read_path = path.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            Here::read_text(&read_path).map_err(|error| format!("{error:#}"))
                        })
                        .await;
                    if this
                        .update(cx, |this, cx| this.receive(revision, result, cx))
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
                result: None,
                revision: 0,
                _pump: pump,
            }
        });
        cx.default_global::<OpenFiles>()
            .0
            .insert(key, file.downgrade());
        file
    }

    pub fn publish(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        self.revision += 1;
        if self.result.as_ref() != Some(&result) {
            self.result = Some(result);
            cx.notify();
        }
    }

    fn receive(&mut self, revision: u64, result: Result<String, String>, cx: &mut Context<Self>) {
        if revision == self.revision {
            self.publish(result, cx);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/file_source.rs"]
mod tests;
