use crate::{model::canvas::Canvas, view::canvas::web::Page};
use artifact::canvas::{comments::Anchor, web::Viewport};
use bezel::{
    gpui::{
        self, AnyElement, App, Bounds, Context, Entity, MouseButton, Pixels, Task, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::widgets::{ButtonStyle, Buttons as _},
};
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

pub(super) struct Pins {
    canvas: Entity<Canvas>,
    pub(super) picking: bool,
    positions: HashMap<u64, Position>,
    refresh: Option<Task<()>>,
    pick: Option<Task<()>>,
    error: Option<String>,
}

impl Pins {
    pub(super) fn new(canvas: Entity<Canvas>, cx: &mut Context<Page>) -> Self {
        cx.observe(&canvas, |_, _, cx| cx.notify()).detach();
        Self {
            canvas,
            picking: false,
            positions: HashMap::new(),
            refresh: None,
            pick: None,
            error: None,
        }
    }

    pub(super) fn cancel_pick(&mut self) {
        self.picking = false;
        self.pick = None;
        self.error = None;
    }
}

impl Page {
    pub(super) fn read_only(&self, cx: &App) -> bool {
        self.owner
            .upgrade()
            .is_none_or(|owner| owner.read(cx).surface.read(cx).editor().options().read_only)
    }

    pub(super) fn toggle_picking(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        self.review.picking = !self.review.picking;
        self.review.error = None;
        self.review.pick = None;
        if self.review.picking && self.interactive {
            self.interactive = false;
            self.present(window, cx);
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn pick_element(&mut self, point: [f32; 2], window: &Window, cx: &mut Context<Self>) {
        let Ok(page) = self.webview() else { return };
        if self.review.pick.is_some() || self.read_only(cx) {
            return;
        }
        let input = json!({ "point": point });
        let task = page
            .read(cx)
            .eval::<Option<Anchor>>(&format!("({ANCHOR})({input})"), EVAL, cx);
        self.review.pick = Some(cx.spawn_in(window, async move |this, cx| {
            let anchor = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.review.pick = None;
                if !this.review.picking || this.browser.as_ref() != Some(&page) {
                    return;
                }
                match anchor {
                    Ok(Some(anchor)) => {
                        this.review.picking = false;
                        let owner = this.owner.clone();
                        let node = this.node.id.clone();
                        cx.defer_in(window, move |_, window, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                owner.draft_comment(node, anchor, window, cx)
                            });
                        });
                    }
                    _ => {
                        this.review.error =
                            Some("Could not select an element. Try another spot.".into())
                    }
                }
                cx.notify();
            });
        }));
    }

    fn review_anchors(&self, cx: &App) -> Vec<(u64, Anchor)> {
        self.review
            .canvas
            .read(cx)
            .record
            .comments
            .iter()
            .filter(|comment| comment.node == self.node.id && !comment.thread.resolved)
            .map(|comment| (comment.thread.id, comment.anchor.clone()))
            .collect()
    }

    fn refresh_pins(&mut self, cx: &mut Context<Self>) {
        if self.review.refresh.is_some() {
            return;
        }
        let anchors = self.review_anchors(cx);
        if anchors.is_empty() {
            self.review.positions.clear();
            return;
        }
        let task = self.webview().ok().and_then(|page| {
            let browser = page.read(cx);
            if !browser.can_evaluate() || browser.is_loading() {
                return None;
            }
            let input =
                json!({ "anchors": anchors.iter().map(|(_, anchor)| anchor).collect::<Vec<_>>() });
            Some(browser.eval::<Vec<Position>>(&format!("({ANCHOR})({input})"), EVAL, cx))
        });
        let timer = cx.background_executor().timer(REFRESH);
        self.review.refresh = Some(cx.spawn(async move |this, cx| {
            let measured = if let Some(task) = task {
                task.await.ok()
            } else {
                None
            };
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.review.refresh = None;
                this.review.positions = measured
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
        let owner = self.owner.clone();
        Theme::of(cx)
            .button(label, ButtonStyle::Secondary, None)
            .id(("web-pin", id))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |owner, cx| owner.show_comment(id, cx));
            })
    }

    pub(super) fn review_surface(
        &mut self,
        body: AnyElement,
        viewport: Viewport,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.refresh_pins(cx);
        let width = viewport.width as f32 * scale;
        let height = viewport.height as f32 * scale;
        let mut overlay = div().relative().w(px(width)).h(px(height)).child(body);
        if self.review.picking && scale > 0. {
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
                                    self.review
                                        .error
                                        .clone()
                                        .unwrap_or_else(|| "Click an element to comment".into()),
                                ),
                        ),
                );
        }
        let mut detached = Vec::new();
        for (i, (id, _)) in self.review_anchors(cx).into_iter().enumerate() {
            let Some(&position) = self.review.positions.get(&id) else {
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
