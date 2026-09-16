use bezel::{
    gpui::{
        self, AnyElement, Context, MouseButton, ObjectFit, Pixels, Point, Render, SharedString,
        Window, div, img, prelude::*, px, relative,
    },
    theme::Theme,
    ui::{popover, tooltip::Tooltip},
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

pub(crate) struct Gallery {
    images: Vec<gpui::ImageSource>,
    focus: gpui::FocusHandle,
    previous_focus: Option<gpui::FocusHandle>,
    selected: usize,
    preview: bool,
    press: Option<Point<Pixels>>,
    drag: f32,
    moved: bool,
    width: Rc<Cell<f32>>,
    preview_width: Rc<Cell<f32>>,
}

impl Gallery {
    pub(crate) fn new(images: Vec<String>, cwd: &Path, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            previous_focus: None,
            images: images
                .into_iter()
                .map(|url| {
                    if let Ok(url) = url::Url::parse(&url) {
                        if let Ok(path) = url.to_file_path() {
                            return Arc::<Path>::from(path).into();
                        }
                        return SharedString::from(url.to_string()).into();
                    }
                    Arc::<Path>::from(cwd.join(url)).into()
                })
                .collect(),
            selected: 0,
            preview: false,
            press: None,
            drag: 0.,
            moved: false,
            width: Default::default(),
            preview_width: Default::default(),
        }
    }

    pub(crate) fn is_preview_open(&self) -> bool {
        self.preview
    }

    fn finish(&mut self, width: f32, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.press.take().is_none() {
            return;
        }
        let threshold = (width * 0.15).clamp(24., 80.);
        if self.drag < -threshold {
            self.selected = (self.selected + 1).min(self.images.len() - 1);
        } else if self.drag > threshold {
            self.selected = self.selected.saturating_sub(1);
        } else if !self.moved && open {
            self.previous_focus = window.focused(cx);
            self.preview = true;
            window.focus(&self.focus, cx);
        }
        self.drag = 0.;
        self.moved = false;
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview = false;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus, cx);
        }
        self.press = None;
        self.drag = 0.;
        cx.notify();
    }

    fn slider(&self, height: Pixels, preview: bool, cx: &mut Context<Self>) -> AnyElement {
        let width = if preview {
            self.preview_width.clone()
        } else {
            self.width.clone()
        };
        let measured = width.clone();
        let released = width.clone();
        let theme = Theme::of(cx).clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id(if preview {
                        "sent-image-preview"
                    } else {
                        "sent-image"
                    })
                    .debug_selector(move || {
                        if preview {
                            "sent-image-preview".into()
                        } else {
                            "sent-image".into()
                        }
                    })
                    .w_full()
                    .h(height)
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
                        cx.listener(move |this, _, window, cx| {
                            this.finish(released.get(), !preview, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.finish(width.get(), false, window, cx);
                        }),
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
                                        img(source.clone())
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
                                .id((if preview { "preview-dot" } else { "image-dot" }, index))
                                .debug_selector(move || {
                                    format!(
                                        "{}-dot-{index}",
                                        if preview { "preview" } else { "image" }
                                    )
                                })
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
            .into_any_element()
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let modal = self.preview.then(|| {
            let theme = Theme::of(cx).clone();
            let this = cx.entity().downgrade();
            let card = crate::view::component::image_preview::frame(
                &theme,
                "sent-image-close",
                self.slider(viewport.height * 0.75, true, cx),
                cx.listener(|this, _, window, cx| this.close(window, cx)),
            )
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => this.close(window, cx),
                    "left" => {
                        this.selected = this.selected.saturating_sub(1);
                        cx.notify();
                    }
                    "right" => {
                        this.selected = (this.selected + 1).min(this.images.len() - 1);
                        cx.notify();
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .w(viewport.width * 0.8);
            popover::modal(
                "sent-image-dialog",
                viewport,
                card.into_any_element(),
                move |_, window, cx| {
                    let _ = this.update(cx, |this, cx| this.close(window, cx));
                },
            )
        });
        div()
            .w_full()
            .child(self.slider(px(240.), false, cx))
            .children(modal)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/transcript_gallery.rs"]
mod tests;
