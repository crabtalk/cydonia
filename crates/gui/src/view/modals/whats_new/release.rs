//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{model::workspace::Workspace, view::kit::icon::kind_icon};
use artifact::kind::Kind;
use bezel::{
    gpui::{Context, Entity, Subscription, Task, Window, div, prelude::*, px, relative},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        tabs::{self, Label, State, Strip, Track},
        widgets::Controls,
    },
};
use std::time::Duration;

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.4";

/// The line under the title.
pub(crate) const HEADLINE: &str = "One strip of tabs per window.";

/// The tabs the window in miniature holds, by their place in [`Body::strip`].
const TABS: [(Kind, &str); 3] = [
    (Kind::Session, "Session"),
    (Kind::Board, "Board"),
    (Kind::Article, "Article"),
];

/// How long the strip holds still between two steps of its loop.
const STEP: Duration = Duration::from_millis(1100);

/// The window in miniature: how tall it stands, the lights' dots, and how far
/// a floating card sits in from its edges.
const WINDOW_HEIGHT: f32 = 140.;
const LIGHT: f32 = 8.;
const INSET: f32 = 4.;

/// One step of the strip's loop, applied to the step before it. The last puts
/// the strip back as it started.
#[derive(Clone, Copy)]
enum Step {
    Front(usize),
    Move(usize, usize),
}

const LOOP: [Step; 5] = [
    Step::Front(1),
    Step::Front(2),
    Step::Move(2, 1),
    Step::Front(0),
    Step::Move(2, 1),
];

/// What follows [`HEADLINE`]: a window in miniature whose strip of tabs steps
/// through its loop, and the switch between the floating and the flat layout,
/// which the miniature is drawn in.
pub(super) struct Body {
    workspace: Entity<Workspace>,
    strip: Strip<usize>,
    reorder: tabs::Reorder<usize>,
    at: usize,
    _loop: Task<()>,
    _watch: Subscription,
}

impl Body {
    pub(super) fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let _watch = cx.observe(&workspace, |_, _, cx| cx.notify());
        let _loop = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(STEP).await;
                if this.update(cx, |this, cx| this.step(cx)).is_err() {
                    return;
                }
            }
        });
        Self {
            workspace,
            strip: (0..TABS.len()).collect(),
            reorder: tabs::Reorder::new(),
            at: 0,
            _loop,
            _watch,
        }
    }

    fn step(&mut self, cx: &mut Context<Self>) {
        match LOOP[self.at] {
            Step::Front(tab) => {
                self.strip.activate(&tab);
            }
            Step::Move(from, to) => self.strip.reorder(from, to),
        }
        self.at = (self.at + 1) % LOOP.len();
        cx.notify();
    }

    /// The window in miniature: its lights and its strip along the top, over
    /// a card when floating and a plain body when flat.
    fn window(&self, floating: bool, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.strip.tabs().iter().map(|&tab| {
            let (kind, name) = TABS[tab];
            let state = match self.strip.active() == Some(&tab) {
                true => State::Focused,
                false => State::Resting,
            };
            let label = Label::new(name).with_icon(kind_icon(kind));
            (
                tab,
                tabs::tab(theme, format!("whats-new-{name}"), label, state),
            )
        });
        let bar = self
            .reorder
            .bar("whats-new-tabs", &self.strip, tabs)
            .on_track(match floating {
                true => Track::Bare,
                false => Track::Flat,
            })
            .on_reorder(cx.listener(|this, moved: &tabs::Move, _, cx| {
                this.strip.reorder(moved.from, moved.to);
                cx.notify();
            }));
        let lights =
            div().flex_none().flex().gap(px(LIGHT / 2.)).children(
                (0..3).map(|_| div().size(px(LIGHT)).rounded_full().bg(theme.text_faint)),
            );
        let top = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(INSET))
            .child(lights)
            .child(div().flex_1().min_w_0().flex().child(bar));
        let body = div()
            .flex_1()
            .bg(bezel::ui::window::content_fill(floating, theme));
        let body = match floating {
            true => body
                .mx(px(INSET))
                .mb(px(INSET))
                .rounded(px(INSET * 2.))
                .border_1()
                .border_color(theme.border),
            false => body.border_t_1().border_color(theme.border),
        };
        div()
            .w(relative(1.))
            .h(px(WINDOW_HEIGHT))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(INSET * 2.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.sidebar_bg())
            .child(top)
            .child(body)
    }
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        let workspace = self.workspace.clone();
        let side = |name: &'static str| {
            div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_muted)
                .child(name)
        };
        div()
            .py_2()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.window(floating, &theme, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(side("Flat"))
                    .child(
                        theme
                            .toggle(floating)
                            .id("whats-new-floating")
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                workspace.update(cx, |workspace, cx| {
                                    workspace.set_floating(!floating, cx)
                                })
                            }),
                    )
                    .child(side("Floating")),
            )
    }
}
