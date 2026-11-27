//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{
    model::workspace::Workspace,
    view::{kit::web::ViewportLook as _, window::root::NewDesign},
};
use artifact::web::Viewport;
use bezel::{
    gpui::{Context, Div, Entity, Subscription, Task, Window, div, prelude::*, px, relative},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons},
    },
};
use std::time::Duration;

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.14";

/// Each page's line under the title, in page order.
pub(super) const HEADLINES: [&str; 1] = [
    "Designs: agents draw pages as live artboards. Review them side by side, switch the device, and open one to zoom in.",
];

/// The artboards the canvas in miniature lays out, by title.
const ARTBOARDS: [&str; 3] = ["Home", "Pricing", "Sign in"];

/// How long the canvas holds one device before the next.
const STEP: Duration = Duration::from_millis(1600);

/// The canvas in miniature: how tall it stands, how tall an artboard is drawn,
/// and how wide a desktop artboard is; the other devices keep their own
/// proportion to it.
const CANVAS_HEIGHT: f32 = 168.;
const ARTBOARD_HEIGHT: f32 = 84.;
const DESKTOP_WIDTH: f32 = 112.;

/// What follows the headline: a design's canvas in miniature, its artboards
/// redrawn at each device in turn, and the way to make one.
pub(super) struct Body {
    workspace: Entity<Workspace>,
    /// Which of [`HEADLINES`]' pages is shown.
    pub(super) page: usize,
    /// Which of [`Viewport::PRESETS`] the artboards are drawn at.
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
                    this.at = (this.at + 1) % Viewport::PRESETS.len();
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
        let designs = self.workspace.read(cx).settings.features.designs;
        let make = designs.then(|| {
            div().flex().justify_center().child(
                theme
                    .button("New design", ButtonStyle::Ghost, None)
                    .id("whats-new-design")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(NewDesign), cx)),
            )
        });
        div()
            .py_2()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(canvas(Viewport::PRESETS[self.at], &theme))
            .children(make)
    }
}

/// The canvas in miniature: the device it is drawn at in its corner, and each
/// of [`ARTBOARDS`] under its title, as wide as `viewport` is against desktop.
fn canvas(viewport: Viewport, theme: &Theme) -> Div {
    let width = DESKTOP_WIDTH * viewport.width as f32 / Viewport::DESKTOP.width as f32;
    let device = div()
        .absolute()
        .top(px(8.))
        .right(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .px(px(6.))
        .py(px(2.))
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .text_style(TextStyle::Caption)
        .text_color(theme.text_muted)
        .child(icons::icon(viewport.icon()).size(px(12.)))
        .child(viewport.title());
    div()
        .relative()
        .w(relative(1.))
        .h(px(CANVAS_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(14.))
        .pt(px(16.))
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.content_bg())
        .children(ARTBOARDS.map(|title| artboard(title, width, theme)))
        .child(device)
}

/// One artboard: its title over a page `width` wide, a bar and two lines of
/// text standing in for what an agent drew.
fn artboard(title: &'static str, width: f32, theme: &Theme) -> Div {
    let line = |share: f32| {
        div()
            .h(px(4.))
            .w(relative(share))
            .rounded_full()
            .bg(theme.element_hover)
    };
    let page = div()
        .w(px(width))
        .h(px(ARTBOARD_HEIGHT))
        .flex()
        .flex_col()
        .gap(px(5.))
        .p(px(6.))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_card)
        .child(
            div()
                .h(px(8.))
                .w_full()
                .rounded(px(2.))
                .bg(theme.element_active),
        )
        .child(line(0.9))
        .child(line(0.6));
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_style(TextStyle::Caption)
                .text_color(theme.text_muted)
                .child(title),
        )
        .child(page)
}
