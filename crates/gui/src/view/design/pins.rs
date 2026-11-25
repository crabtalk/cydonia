use crate::view::design::DesignView;
use artifact::{comment::Anchor, design::Page};
use bezel::{
    gpui::{
        self, AnyElement, Bounds, Context, Entity, MouseButton, Pixels, Size, Task, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::widgets::{ButtonStyle, Buttons as _},
};
use browser::WebView;
use serde::Deserialize;
use serde_json::json;
use std::{cell::Cell, collections::HashMap, rc::Rc, time::Duration};

const ANCHOR: &str = include_str!("anchor.js");
const EVAL: Duration = Duration::from_secs(2);
const REFRESH: Duration = Duration::from_millis(350);
const PIN_WIDTH: f32 = 30.;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
struct Position {
    attached: bool,
    point: Option<[f32; 2]>,
}

#[derive(Default)]
pub(super) struct Pins {
    pub(super) picking: bool,
    positions: HashMap<u64, Position>,
    refresh: Option<Task<()>>,
    pick: Option<Task<()>>,
    error: Option<String>,
}

impl DesignView {
    pub(super) fn toggle_picking(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pins.picking = !self.pins.picking;
        self.pins.error = None;
        self.pins.pick = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn pick_element(&mut self, point: [f32; 2], window: &Window, cx: &mut Context<Self>) {
        let Some(page) = self.open_browser() else {
            return;
        };
        let Some(id) = self.open.clone() else { return };
        if self.pins.pick.is_some() {
            return;
        }
        let input = json!({ "point": point, "base": self.comment_base(&id) });
        let task = page
            .read(cx)
            .eval::<Option<Anchor>>(&format!("({ANCHOR})({input})"), EVAL, cx);
        self.pins.pick = Some(cx.spawn_in(window, async move |this, cx| {
            let anchor = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.pins.pick = None;
                if !this.pins.picking || this.open_browser().as_ref() != Some(&page) {
                    return;
                }
                match anchor {
                    Ok(Some(anchor)) => {
                        this.pins.picking = false;
                        this.draft_comment(id, anchor, window, cx);
                    }
                    _ => {
                        this.pins.error =
                            Some("Could not select an element. Try another spot.".into())
                    }
                }
                cx.notify();
            });
        }));
    }

    fn review_anchors(page: &Page) -> Vec<(u64, Anchor)> {
        page.comments
            .iter()
            .filter(|comment| !comment.thread.resolved)
            .map(|comment| (comment.thread.id, comment.anchor.clone()))
            .collect()
    }

    fn refresh_pins(&mut self, page: &Page, browser: &Entity<WebView>, cx: &mut Context<Self>) {
        if self.pins.refresh.is_some() {
            return;
        }
        let anchors = Self::review_anchors(page);
        if anchors.is_empty() {
            self.pins.positions.clear();
            return;
        }
        let surface = browser.clone();
        let browser = browser.read(cx);
        let task = if browser.can_evaluate() && !browser.is_loading() {
            let input = json!({
                "anchors": anchors.iter().map(|(_, anchor)| anchor).collect::<Vec<_>>(),
                "base": self.comment_base(&page.id),
            });
            Some(browser.eval::<Vec<Position>>(&format!("({ANCHOR})({input})"), EVAL, cx))
        } else {
            None
        };
        let timer = cx.background_executor().timer(REFRESH);
        self.pins.refresh = Some(cx.spawn(async move |this, cx| {
            let measured = if let Some(task) = task {
                task.await.ok()
            } else {
                None
            };
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.pins.refresh = None;
                if this.open_browser().as_ref() != Some(&surface) {
                    return;
                }
                this.pins.positions = measured
                    .into_iter()
                    .flatten()
                    .zip(anchors.into_iter().map(|(id, _)| id))
                    .map(|(position, id)| (id, position))
                    .collect();
                cx.notify();
            });
        }));
    }

    fn pin(&self, id: u64, label: String, cx: &Context<Self>) -> impl IntoElement {
        Theme::of(cx)
            .button(label, ButtonStyle::Secondary, None)
            .id(("design-pin", id))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| this.show_comment(id, cx)))
    }

    pub(super) fn detached(&self, id: u64) -> bool {
        self.pins
            .positions
            .get(&id)
            .is_some_and(|position| !position.attached)
    }

    pub(super) fn review_surface(
        &mut self,
        browser: Entity<WebView>,
        page: &Page,
        fit: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.refresh_pins(page, &browser, cx);
        let width = fit.width.as_f32();
        let height = fit.height.as_f32();
        let mut overlay = div().relative().w(fit.width).h(fit.height).child(browser);
        if self.pins.picking && width > 0. && height > 0. {
            let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
            overlay = overlay
                .child(
                    gpui::canvas(
                        {
                            let bounds = bounds.clone();
                            move |area, _, _| bounds.set(area)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .cursor_crosshair()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                let local = event.position - bounds.get().origin;
                                this.pick_element(
                                    [local.x.as_f32() / width, local.y.as_f32() / height],
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(8.))
                                .left(px(8.))
                                .bg(Theme::of(cx).surface_card)
                                .px(px(8.))
                                .py(px(4.))
                                .text_style(TextStyle::Caption)
                                .child(
                                    self.pins
                                        .error
                                        .clone()
                                        .unwrap_or_else(|| "Click an element to comment".into()),
                                ),
                        ),
                );
        }
        let mut detached = Vec::new();
        for (i, (id, _)) in Self::review_anchors(page).into_iter().enumerate() {
            let Some(&position) = self.pins.positions.get(&id) else {
                continue;
            };
            let label = (i + 1).to_string();
            if let Some([x, y]) = position.point {
                overlay = overlay.child(
                    div()
                        .absolute()
                        .left(px((width * x).clamp(0., (width - PIN_WIDTH).max(0.))))
                        .top(px(
                            (height * y - PIN_WIDTH / 2.).clamp(0., (height - PIN_WIDTH).max(0.))
                        ))
                        .child(self.pin(id, label, cx)),
                );
            } else if !position.attached {
                detached.push(self.pin(id, format!("{label} Detached"), cx));
            }
        }
        overlay
            .when(!detached.is_empty(), |overlay| {
                overlay.child(
                    div()
                        .absolute()
                        .left(px(4.))
                        .right(px(4.))
                        .top(px(4.))
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .children(detached),
                )
            })
            .into_any_element()
    }
}
