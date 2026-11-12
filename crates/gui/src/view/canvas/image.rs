use crate::{
    model::workspace::references,
    view::component::file::pictures::{Assets, Pictures},
};
use bezel::gpui::{
    self, App, AppContext as _, Context, Entity, ObjectFit, Render, SharedString, Subscription,
    Window, div, img, prelude::*, px,
};
use canvas::{
    kind::{self, Card, Chrome},
    model::Node,
};
use std::path::{Path, PathBuf};

pub(super) fn render(
    node: &Node,
    card: &Card,
    root: &Path,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    kind::chrome(Chrome::Bare, node, card.zoom, cx)
        .relative()
        .overflow_hidden()
        .children(Image::for_node(node, root, window, cx))
        .into_any_element()
}

struct Image {
    path: PathBuf,
    pictures: Entity<Pictures>,
    _watch: Subscription,
}

impl Image {
    fn for_node(
        node: &Node,
        root: &Path,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<Self>> {
        let workspace = references::current(cx)?;
        let path = root.join(node.file.as_deref()?);
        let key: SharedString = format!("canvas-image-{}-{}", node.id, path.display()).into();
        Some(window.use_keyed_state(key, cx, |_, cx| {
            let base = path.parent().unwrap_or(root).to_path_buf();
            let pictures = cx.new(|cx| {
                Pictures::new(
                    base.clone(),
                    Assets::Dir(base.join("assets")),
                    workspace.downgrade(),
                    cx,
                )
            });
            let watch = cx.observe(&pictures, |_, _, cx| cx.notify());
            Self {
                path,
                pictures,
                _watch: watch,
            }
        }))
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
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::AnyElement> {
        if !card.selected || !node.file.as_deref().is_some_and(markdown::is_image) {
            return None;
        }
        let image = Image::for_node(node, root, window, cx)?;
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
