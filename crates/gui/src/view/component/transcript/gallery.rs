use crate::view::component::image_preview::Preview;
use bezel::{
    gpui::{
        self, Context, Entity, MouseButton, ObjectFit, Pixels, Point, Render, SharedString, Window,
        div, img, prelude::*, px, relative,
    },
    theme::Theme,
    ui::tooltip::Tooltip,
};
use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

pub(crate) fn document(text: &str) -> (markdown::Doc, Vec<String>) {
    let mut doc = markdown::parse(text);
    let mut images = Vec::new();
    doc.blocks.retain(|block| {
        if let markdown::BlockKind::Image { url, .. } = &block.kind {
            images.push(url.clone());
            false
        } else {
            true
        }
    });
    (doc, images)
}

/// Where an image URL in a message points: a `file:` URL or a path against
/// the session's directory reads from disk, any other URL is fetched.
pub(crate) fn source(url: &str, cwd: &Path) -> gpui::ImageSource {
    if let Ok(url) = url::Url::parse(url) {
        if let Some(path) = crate::model::file_url::to_path(&url) {
            return Arc::<Path>::from(path).into();
        }
        return SharedString::from(url.to_string()).into();
    }
    Arc::<Path>::from(cwd.join(url)).into()
}

pub(crate) struct Gallery {
    images: Vec<gpui::ImageSource>,
    selected: usize,
    preview: Entity<Preview>,
    press: Option<Point<Pixels>>,
    drag: f32,
    moved: bool,
    width: Rc<Cell<f32>>,
}

impl Gallery {
    pub(crate) fn new(images: Vec<String>, cwd: &Path, cx: &mut Context<Self>) -> Self {
        let images: Vec<_> = images.iter().map(|url| source(url, cwd)).collect();
        let preview = cx.new(|cx| Preview::new(images.clone(), cx));
        // Closing the preview leaves the strip on the image it was showing.
        cx.observe(&preview, |this, preview, cx| {
            let preview = preview.read(cx);
            if !preview.open && this.selected != preview.selected {
                this.selected = preview.selected;
                cx.notify();
            }
        })
        .detach();
        Self {
            images,
            selected: 0,
            preview,
            press: None,
            drag: 0.,
            moved: false,
            width: Default::default(),
        }
    }

    pub(crate) fn is_preview_open(&self, cx: &gpui::App) -> bool {
        self.preview.read(cx).open
    }

    fn finish(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.press.take().is_none() {
            return;
        }
        let threshold = (self.width.get() * 0.15).clamp(24., 80.);
        if self.drag < -threshold {
            self.selected = (self.selected + 1).min(self.images.len() - 1);
        } else if self.drag > threshold {
            self.selected = self.selected.saturating_sub(1);
        } else if !self.moved && open {
            let selected = self.selected;
            self.preview
                .update(cx, |preview, cx| preview.show(selected, window, cx));
        }
        self.drag = 0.;
        self.moved = false;
        cx.notify();
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let measured = self.width.clone();
        let theme = Theme::of(cx).clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id("sent-image")
                    .debug_selector(|| "sent-image".into())
                    .w_full()
                    .h(px(240.))
                    .relative()
                    .overflow_hidden()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.press = Some(event.position);
                            this.drag = 0.;
                            this.moved = false;
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if let Some(start) = this.press {
                            let delta = f32::from(event.position.x - start.x);
                            this.drag = if this.images.len() > 1 { delta } else { 0. };
                            this.moved |=
                                delta.abs() > 5. || (event.position.y - start.y).abs() > px(5.);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.finish(true, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.finish(false, window, cx)),
                    )
                    .children(
                        self.images
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| index.abs_diff(self.selected) <= 1)
                            .map(|(index, source)| {
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(relative(index as f32 - self.selected as f32))
                                    .ml(px(self.drag))
                                    .size_full()
                                    .child(
                                        // Pinned to the slide's edges: under
                                        // `size_full` alone the height takes
                                        // the picture's own ratio and is cut.
                                        img(source.clone())
                                            .absolute()
                                            .inset_0()
                                            .size_full()
                                            .object_fit(ObjectFit::Contain),
                                    )
                            }),
                    )
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| measured.set(f32::from(bounds.size.width)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
            .when(self.images.len() > 1, |gallery| {
                gallery.child(
                    div()
                        .flex()
                        .justify_center()
                        .children((0..self.images.len()).map(|index| {
                            div()
                                .id(("image-dot", index))
                                .debug_selector(move || format!("image-dot-{index}"))
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .tooltip(move |window, cx| {
                                    Tooltip::text(format!("Image {}", index + 1), window, cx)
                                })
                                .child(div().size(px(6.)).rounded_full().bg(
                                    if index == self.selected {
                                        theme.text
                                    } else {
                                        theme.text_faint.opacity(0.4)
                                    },
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected = index;
                                    this.drag = 0.;
                                    this.press = None;
                                    cx.notify();
                                }))
                        })),
                )
            })
            .child(self.preview.clone())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/transcript_gallery.rs"]
mod tests;
