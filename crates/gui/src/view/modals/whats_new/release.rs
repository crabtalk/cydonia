//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{model::workspace::Workspace, view::kit::icon::kind_icon};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        AnyElement, App, Context, Div, Entity, Subscription, Task, Window, div, prelude::*, px,
        relative,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tabs::{self, State},
        widgets::Controls,
    },
};
use std::time::Duration;

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.6";

/// Each page's line under the title, in page order.
pub(super) const HEADLINES: [&str; 1] =
    ["The pane you are in stands out, and Esc steps back out of it."];

/// The panes the window holds, side by side.
const PANES: [(Kind, &str); 2] = [(Kind::Board, "Board"), (Kind::Session, "Session")];

/// Where the focus is at each step of the loop: a pane, by its place in
/// [`PANES`], or the window after Esc.
const LOOP: [Option<usize>; 3] = [Some(0), Some(1), None];

/// How long the focus holds still between two steps of its loop.
const STEP: Duration = Duration::from_millis(1100);

/// The window in miniature: how tall it stands, the lights' dots, and how far
/// a floating card sits in from its edges and from the card beside it.
const WINDOW_HEIGHT: f32 = 140.;
const LIGHT: f32 = 8.;
const INSET: f32 = 4.;

/// What follows the headline: a window in miniature whose focus steps from
/// one pane to the other and out, drawn in the floating or the flat layout,
/// and the switch between the two.
pub(super) struct Body {
    workspace: Entity<Workspace>,
    /// Which of [`HEADLINES`]' pages is shown.
    pub(super) page: usize,
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
                let stepped = this.update(cx, |this, cx| {
                    this.at = (this.at + 1) % LOOP.len();
                    cx.notify();
                });
                if stepped.is_err() {
                    return;
                }
            }
        });
        Self {
            workspace,
            page: 0,
            at: 0,
            _loop,
            _watch,
        }
    }
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        let window = window(floating, LOOP[self.at], &theme, cx);
        let workspace = self.workspace.clone();
        let switch = theme
            .toggle(floating)
            .id("whats-new-floating")
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                workspace.update(cx, |workspace, cx| workspace.set_floating(!floating, cx))
            });
        div()
            .py_2()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(window)
            .child(row([
                side("Flat", &theme),
                switch.into_any_element(),
                side("Floating", &theme),
            ]))
    }
}

/// A window in miniature: its lights along the top and [`PANES`] side by side
/// under them. Floating, each pane is a card, the focused one raised; flat,
/// each pane's title stands on a tab's chip, the focused one lit.
fn window(floating: bool, focus: Option<usize>, theme: &Theme, cx: &App) -> Div {
    let lights = div()
        .flex_none()
        .flex()
        .gap(px(LIGHT / 2.))
        .px(px(10.))
        .py(px(10.))
        .children((0..3).map(|_| div().size(px(LIGHT)).rounded_full().bg(theme.text_faint)));
    let panes = PANES.iter().enumerate().map(|(at, (kind, name))| {
        let focused = focus == Some(at);
        let title = line(kind_icon(*kind), name, theme);
        match floating {
            true => match focused {
                true => bezel::ui::window::content(true, cx),
                false => bezel::ui::window::resting(cx),
            }
            .flex_1()
            .p(px(8.))
            .child(title),
            false => div()
                .flex_1()
                .p(px(6.))
                .when(at > 0, |pane| pane.border_l_1().border_color(theme.border))
                .child(
                    tabs::chip(
                        theme,
                        match focused {
                            true => State::Focused,
                            false => State::Front,
                        },
                    )
                    .child(title),
                ),
        }
    });
    let body = div().flex_1().min_h_0().flex().flex_row();
    let body = match floating {
        true => body
            .gap(px(INSET))
            .px(px(INSET))
            .pb(px(INSET))
            .children(panes),
        false => body
            .bg(bezel::ui::window::content_fill(false, theme))
            .border_t_1()
            .border_color(theme.border)
            .children(panes),
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
        .child(lights)
        .child(body)
}

/// A pane's title in miniature: its entry's mark and name.
fn line(icon: icons::Icon, name: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .text_style(TextStyle::Caption)
        .child(icons::icon(icon).size(px(12.)).text_color(theme.text_muted))
        .child(name)
}

/// A switch's name beside it.
fn side(name: &'static str, theme: &Theme) -> AnyElement {
    div()
        .text_style(TextStyle::Callout)
        .text_color(theme.text_muted)
        .child(name)
        .into_any_element()
}

/// The switch row, centred under the window.
fn row(children: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .children(children)
}
