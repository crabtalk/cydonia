use crate::{
    model::workspace::references,
    view::component::file::pictures::{Assets, Pictures},
};
use bezel::gpui::{
    self, App, AppContext as _, Context, Entity, ObjectFit, Render, SharedString, Subscription,
    WeakEntity, Window, div, img, prelude::*, px,
};
use canvas::{
    kind::{self, Card, Chrome},
    model::Node,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

type ImageCache = HashMap<(String, PathBuf), WeakEntity<Image>>;

#[derive(Clone, Default)]
pub(super) struct Images(Rc<RefCell<ImageCache>>);

pub(super) fn render(
    node: &Node,
    card: &Card,
    root: &Path,
    images: &Images,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    kind::chrome(Chrome::Bare, node, card.zoom, cx)
        .relative()
        .overflow_hidden()
        .children(images.for_node(node, root, window, cx))
        .into_any_element()
}

struct Image {
    path: PathBuf,
    pictures: Entity<Pictures>,
    _watch: Subscription,
}

impl Images {
    fn for_node(
        &self,
        node: &Node,
        root: &Path,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<Image>> {
        let workspace = references::current(cx)?;
        let path = root.join(node.file.as_deref()?);
        let key: SharedString = format!("canvas-image-{}-{}", node.id, path.display()).into();
        let held = window.use_keyed_state(key, cx, |_, cx| {
            let identity = (node.id.clone(), path.clone());
            let mut images = self.0.borrow_mut();
            images.retain(|_, image| image.is_upgradable());
            if let Some(image) = images.get(&identity).and_then(WeakEntity::upgrade) {
                return image;
            }
            let base = path.parent().unwrap_or(root).to_path_buf();
            let pictures = cx.new(|cx| {
                Pictures::new(
                    base.clone(),
                    Assets::Dir(base.join("assets")),
                    workspace.downgrade(),
                    cx,
                )
            });
            let image = cx.new(|cx| Image {
                path,
                _watch: cx.observe(&pictures, |_, _, cx| cx.notify()),
                pictures,
            });
            images.insert(identity, image.downgrade());
            image
        });
        Some(held.read(cx).clone())
    }
}

impl Render for Image {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        img(self.path.clone())
            .size_full()
            .object_fit(ObjectFit::Contain)
    }
}

pub(super) struct Opener {
    image: Entity<Image>,
    _watch: Subscription,
}

impl Opener {
    pub(super) fn for_node(
        node: &Node,
        card: &Card,
        root: &Path,
        images: &Images,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::AnyElement> {
        if !card.selected || !node.file.as_deref().is_some_and(markdown::is_image) {
            return None;
        }
        let image = images.for_node(node, root, window, cx)?;
        let key: SharedString = format!("canvas-image-opener-{}", image.entity_id()).into();
        let opener = window.use_keyed_state(key, cx, |_, cx| {
            let watch = cx.observe(&image, |_, _, cx| cx.notify());
            Self {
                image,
                _watch: watch,
            }
        });
        Some(opener.into_any_element())
    }
}

impl Render for Opener {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.read(cx);
        let overlay = Pictures::overlay(&image.pictures);
        let path = image.path.to_string_lossy().into_owned();
        let tools = overlay(0, &path, window, cx);
        div()
            .id("canvas-image-opener")
            .absolute()
            .bottom(px(4.))
            .right(px(4.))
            .cursor_pointer()
            .block_mouse_except_scroll()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .children(tools)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/canvas_images.rs"]
mod tests;
