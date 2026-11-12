use crate::model::{
    disk::{Files as _, Here},
    watch::path::Changes,
    workspace::Workspace,
};
use bezel::gpui::{App, AppContext as _, Context, Entity, Global, Subscription, Task, WeakEntity};
use std::{collections::HashMap, path::PathBuf};

#[derive(Default)]
struct OpenFiles(HashMap<PathBuf, WeakEntity<File>>);
impl Global for OpenFiles {}

pub struct File {
    pub result: Option<Result<String, String>>,
    revision: u64,
    _changes: Entity<Changes>,
    _watch: Subscription,
    _load: Option<Task<()>>,
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
            let changes = Changes::open(path.clone(), workspace, cx);
            let watch = cx.observe(&changes, {
                let path = path.clone();
                move |this, _, cx| this.load(path.clone(), cx)
            });
            let mut file = Self {
                result: None,
                revision: 0,
                _changes: changes,
                _watch: watch,
                _load: None,
            };
            file.load(path, cx);
            file
        });
        cx.default_global::<OpenFiles>()
            .0
            .insert(key, file.downgrade());
        file
    }

    fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let revision = self.revision;
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Here::read_text(&path).map_err(|error| format!("{error:#}")) })
                .await;
            let _ = this.update(cx, |this, cx| this.receive(revision, result, cx));
        }));
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
