use crate::model::{
    watch::path::{Changes, Stamp},
    workspace::Workspace,
};
use bezel::gpui::{self, App, AppContext as _, Context, Entity, Global, Subscription, WeakEntity};
use std::{collections::HashMap, path::PathBuf};

#[derive(Default)]
struct OpenPictures(HashMap<PathBuf, WeakEntity<Picture>>);
impl Global for OpenPictures {}

#[derive(Default)]
struct Loaded(HashMap<PathBuf, Option<Stamp>>);
impl Global for Loaded {}

pub(super) struct Picture {
    changes: Entity<Changes>,
    _watch: Subscription,
}

impl Picture {
    pub(super) fn open(
        path: PathBuf,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Entity<Self> {
        let changes = Changes::open(path, workspace, cx);
        let path = changes.read(cx).path.clone();
        let pictures = &mut cx.default_global::<OpenPictures>().0;
        pictures.retain(|_, picture| picture.is_upgradable());
        if let Some(picture) = pictures.get(&path).and_then(WeakEntity::upgrade) {
            return picture;
        }
        let key = path.clone();
        let picture = cx.new(|cx: &mut Context<Self>| {
            let watch = cx.observe(&changes, |this, _, cx| this.reload(cx));
            let picture = Self {
                changes,
                _watch: watch,
            };
            picture.reload(cx);
            picture
        });
        cx.default_global::<OpenPictures>()
            .0
            .insert(key, picture.downgrade());
        picture
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let changes = self.changes.read(cx);
        if !changes.ready {
            return;
        }
        let (path, stamp) = (changes.path.clone(), changes.stamp);
        let loaded = &mut cx.default_global::<Loaded>().0;
        let changed = loaded.get(&path).is_some_and(|last| *last != stamp);
        loaded.insert(path.clone(), stamp);
        if changed {
            let source = gpui::ImageSource::from(path);
            source.remove_asset(cx);
            if let gpui::ImageSource::Resource(resource) = &source {
                cx.fetch_asset::<gpui::ImgResourceLoader>(resource);
            }
            cx.notify();
        }
    }
}
