//! Shared image-preview frame and controls.
use bezel::{
    gpui::{
        self, AnyElement, Context, CursorStyle, Empty, MouseButton, ObjectFit, Pixels, Render,
        Window, div, img, prelude::*, px, relative,
    },
    theme::Theme,
    ui::{icons, popover, surface, tooltip::Tooltip},
};
use std::{cell::Cell, rc::Rc};

pub(super) fn disc(
    theme: &Theme,
    id: impl Into<gpui::ElementId>,
    side: f32,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(side))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.solid)
        .cursor_pointer()
        .hover(|button| button.opacity(0.85))
        .child(
            icons::icon(icons::notifications::X)
                .size(px(side * 0.6))
                .text_color(theme.on_solid),
        )
}

pub(super) fn frame(
    theme: &Theme,
    close_id: &'static str,
    content: impl IntoElement,
    on_close: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Div {
    div()
        .relative()
        .child(content)
        // Images paint in their own layer; keep the close control above them.
        .child(surface::layered(
            disc(theme, close_id, 24.)
                .debug_selector(move || close_id.into())
                .absolute()
                .top(px(10.))
                .right(px(10.))
                .tooltip(|window, cx| Tooltip::text("Close", window, cx))
                .on_click(on_close),
        ))
}

const ZOOM_MIN: f32 = 1.;
const ZOOM_MAX: f32 = 8.;
const ZOOM_STEP: f32 = 1.25;

/// A modal over a run of images: arrows and dots page through them, the wheel,
/// the buttons and `+` `-` `0` scale the one on show, and a drag pans it once
/// it is larger than its frame.
pub(crate) struct Preview {
    pub(crate) images: Vec<gpui::ImageSource>,
    pub(crate) selected: usize,
    pub(crate) open: bool,
    pub(crate) zoom: f32,
    pan: gpui::Point<Pixels>,
    press: Option<gpui::Point<Pixels>>,
    drag: f32,
    focus: gpui::FocusHandle,
    previous_focus: Option<gpui::FocusHandle>,
    /// The frame's size as last painted, window pixels.
    frame: Rc<Cell<gpui::Size<Pixels>>>,
}

impl Preview {
    pub(crate) fn new(images: Vec<gpui::ImageSource>, cx: &mut Context<Self>) -> Self {
        Self {
            images,
            selected: 0,
            open: false,
            zoom: ZOOM_MIN,
            pan: gpui::Point::default(),
            press: None,
            drag: 0.,
            focus: cx.focus_handle(),
            previous_focus: None,
            frame: Default::default(),
        }
    }

    pub(crate) fn show(&mut self, selected: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.images.is_empty() {
            return;
        }
        if !self.open {
            self.previous_focus = window.focused(cx);
        }
        self.open = true;
        self.select(selected, cx);
        window.focus(&self.focus, cx);
    }

    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(focus) = self.previous_focus.take() {
            window.focus(&focus, cx);
        }
        self.press = None;
        self.drag = 0.;
        cx.notify();
    }

    pub(crate) fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index.min(self.images.len().saturating_sub(1));
        self.zoom = ZOOM_MIN;
        self.pan = gpui::Point::default();
        self.press = None;
        self.drag = 0.;
        cx.notify();
    }

    pub(crate) fn zoom_to(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let ratio = zoom / self.zoom;
        self.zoom = zoom;
        self.pan = self.clamped(gpui::point(self.pan.x * ratio, self.pan.y * ratio));
        cx.notify();
    }

    /// Keep the scaled picture over the whole frame: no pan at 1×, and never
    /// past the edge that would show the backdrop.
    fn clamped(&self, pan: gpui::Point<Pixels>) -> gpui::Point<Pixels> {
        let frame = self.frame.get();
        let room = |side: Pixels| side * ((self.zoom - 1.) / 2.);
        let (x, y) = (room(frame.width), room(frame.height));
        gpui::point(pan.x.clamp(-x, x), pan.y.clamp(-y, y))
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        if self.press.take().is_none() {
            return;
        }
        let threshold = (f32::from(self.frame.get().width) * 0.15).clamp(24., 80.);
        if self.drag < -threshold {
            self.select(self.selected + 1, cx);
        } else if self.drag > threshold {
            self.select(self.selected.saturating_sub(1), cx);
        }
        self.drag = 0.;
        cx.notify();
    }

    fn stage(&self, height: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let frame = self.frame.clone();
        let size = frame.get();
        let zoomed = self.zoom > ZOOM_MIN;
        let (width, tall) = (size.width * self.zoom, size.height * self.zoom);
        div()
            .id("image-preview")
            .debug_selector(|| "image-preview".into())
            .w_full()
            .h(height)
            .relative()
            .overflow_hidden()
            .rounded(px(8.))
            .when(zoomed, |stage| {
                stage.cursor(if self.press.is_some() {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    this.press = Some(event.position);
                    this.drag = 0.;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                let Some(start) = this.press else {
                    return;
                };
                if this.zoom > ZOOM_MIN {
                    this.pan = this.clamped(this.pan + (event.position - start));
                    this.press = Some(event.position);
                } else if this.images.len() > 1 {
                    this.drag = f32::from(event.position.x - start.x);
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.release(cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.release(cx)),
            )
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                let delta = f32::from(event.delta.pixel_delta(px(16.)).y);
                this.zoom_to(this.zoom * (delta / 200.).exp(), cx);
                cx.stop_propagation();
            }))
            .children(
                self.images
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| index.abs_diff(self.selected) <= 1)
                    .map(|(index, source)| {
                        let slide = div()
                            .absolute()
                            .top_0()
                            .left(relative(index as f32 - self.selected as f32))
                            .ml(px(self.drag))
                            .size_full();
                        let picture = img(source.clone()).object_fit(ObjectFit::Contain);
                        if zoomed && index == self.selected {
                            slide.child(
                                picture
                                    .absolute()
                                    .left((size.width - width) / 2. + self.pan.x)
                                    .top((size.height - tall) / 2. + self.pan.y)
                                    .w(width)
                                    .h(tall),
                            )
                        } else {
                            // Pinned to the slide's edges: under `size_full`
                            // alone the height takes the picture's own ratio
                            // and is cut.
                            slide.child(picture.absolute().inset_0().size_full())
                        }
                    }),
            )
            .child(
                gpui::canvas(move |bounds, _, _| frame.set(bounds.size), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }

    fn controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let button = |id: &'static str, icon: &'static [u8], tip: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|button| button.bg(theme.surface_raised))
                .tooltip(move |window, cx| Tooltip::text(tip, window, cx))
                .child(icons::icon(icon).size(px(14.)).text_color(theme.text_muted))
        };
        let dots = (self.images.len() > 1).then(|| {
            div().flex().children((0..self.images.len()).map(|index| {
                div()
                    .id(("preview-dot", index))
                    .debug_selector(move || format!("preview-dot-{index}"))
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .tooltip(move |window, cx| {
                        Tooltip::text(format!("Image {}", index + 1), window, cx)
                    })
                    .child(
                        div()
                            .size(px(6.))
                            .rounded_full()
                            .bg(if index == self.selected {
                                theme.text
                            } else {
                                theme.text_faint.opacity(0.4)
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.select(index, cx)))
            }))
        });
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(div().flex_1().children(dots))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .text_color(theme.text_muted)
                    .child(
                        button("preview-zoom-out", icons::photography::ZoomOut, "Zoom out")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.zoom_to(this.zoom / ZOOM_STEP, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("preview-zoom-reset")
                            .min_w(px(44.))
                            .flex()
                            .justify_center()
                            .cursor_pointer()
                            .tooltip(|window, cx| Tooltip::text("Fit", window, cx))
                            .child(format!("{:.0}%", self.zoom * 100.))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_to(ZOOM_MIN, cx))),
                    )
                    .child(
                        button("preview-zoom-in", icons::photography::ZoomIn, "Zoom in").on_click(
                            cx.listener(|this, _, _, cx| this.zoom_to(this.zoom * ZOOM_STEP, cx)),
                        ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return Empty.into_any_element();
        }
        let viewport = window.viewport_size();
        let theme = Theme::of(cx).clone();
        let this = cx.entity().downgrade();
        let content = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(self.stage(viewport.height * 0.75, cx))
            .child(self.controls(cx));
        let card = frame(
            &theme,
            "image-preview-close",
            content,
            cx.listener(|this, _, window, cx| this.close(window, cx)),
        )
        .track_focus(&self.focus)
        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
            match event.keystroke.key.as_str() {
                "escape" => this.close(window, cx),
                "left" => this.select(this.selected.saturating_sub(1), cx),
                "right" => this.select(this.selected + 1, cx),
                "=" | "+" => this.zoom_to(this.zoom * ZOOM_STEP, cx),
                "-" => this.zoom_to(this.zoom / ZOOM_STEP, cx),
                "0" => this.zoom_to(ZOOM_MIN, cx),
                _ => return,
            }
            cx.stop_propagation();
        }))
        .w(viewport.width * 0.8);
        popover::modal(
            "image-preview-dialog",
            viewport,
            card.into_any_element(),
            move |_, window, cx| {
                let _ = this.update(cx, |this, cx| this.close(window, cx));
            },
        )
    }
}
