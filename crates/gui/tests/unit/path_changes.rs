use crate::model::watch::path::{Changes, OpenPaths};
use bezel::gpui::{App, AppContext as _, Context, Entity, Task};
use std::path::Path;

pub(crate) fn open(path: &Path, cx: &mut App) -> Entity<Changes> {
    let path = std::path::absolute(path).unwrap();
    let changes = cx.new(|_: &mut Context<Changes>| Changes {
        path: path.clone(),
        stamp: None,
        ready: false,
        _pump: Task::ready(()),
    });
    cx.default_global::<OpenPaths>()
        .0
        .insert(path, changes.downgrade());
    publish(&changes, cx);
    changes
}

pub(crate) fn publish(changes: &Entity<Changes>, cx: &mut App) {
    changes.update(cx, |changes, cx| {
        changes.stamp = std::fs::metadata(&changes.path)
            .ok()
            .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
        changes.ready = true;
        cx.notify();
    });
}
