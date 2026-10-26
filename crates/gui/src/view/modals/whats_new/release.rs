//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{
    model::workspace::Workspace,
    view::{kit::icon::kind_icon, window::root::EditAgent},
};
use artifact::kind::Kind;
use bezel::{
    agent::{Face, avatar::Motion, mascot},
    gpui::{
        AnyElement, Context, Div, Entity, Subscription, Task, Window, div, prelude::*, px, relative,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons},
    },
};
use std::time::Duration;

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.7";

/// Each page's line under the title, in page order.
pub(super) const HEADLINES: [&str; 1] =
    ["Name an agent, give it a face, and keep its sessions under it in the sidebar."];

/// The agents the sidebar in miniature lists, each by name and its sessions'
/// titles.
const AGENTS: [(&str, [&str; 2]); 2] = [
    ("Ada", ["Plan the release", "Fix the flaky test"]),
    ("Turing", ["Review the parser", "Answer the issue"]),
];

/// Which of [`AGENTS`] are open at each step of the loop; a folded one sleeps.
const LOOP: [[bool; 2]; 3] = [[true, false], [true, true], [false, true]];

/// How long the sidebar holds still between two steps of its loop.
const STEP: Duration = Duration::from_millis(1400);

/// The sidebar in miniature: how tall it stands, and how wide a blob is drawn.
const SIDEBAR_HEIGHT: f32 = 168.;
const BLOB: f32 = 14.;

/// What follows the headline: the sidebar's Agents in miniature, an agent
/// folding to sleep and waking open on a loop, and the way to make one.
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
        let sessions = self.workspace.read(cx).settings.features.sessions;
        let make = sessions.then(|| {
            div().flex().justify_center().child(
                theme
                    .button("New agent…", ButtonStyle::Ghost, None)
                    .id("whats-new-agent")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(
                            Box::new(EditAgent {
                                id: None,
                                from: None,
                            }),
                            cx,
                        )
                    }),
            )
        });
        div()
            .py_2()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(sidebar(LOOP[self.at], &theme))
            .children(make)
    }
}

/// The sidebar in miniature: the Agents heading, then each of [`AGENTS`] with
/// its blob, its sessions under it where `open` says so.
fn sidebar(open: [bool; 2], theme: &Theme) -> Div {
    let groups = AGENTS
        .iter()
        .zip(open)
        .map(|((name, sessions), open)| agent(name, sessions, open, theme));
    div()
        .w(relative(1.))
        .h(px(SIDEBAR_HEIGHT))
        .flex()
        .flex_col()
        .gap(px(2.))
        .p(px(10.))
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar_bg())
        .child(
            div()
                .pb(px(4.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("Agents"),
        )
        .children(groups)
}

/// One agent: its blob and name, asleep while folded, and its sessions under
/// it while open.
fn agent(name: &'static str, sessions: &[&'static str], open: bool, theme: &Theme) -> AnyElement {
    let face = Face::from(name).motion(match open {
        true => Motion::default(),
        false => Motion::ASLEEP,
    });
    let head = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .py(px(3.))
        .text_style(TextStyle::Callout)
        .text_color(theme.text)
        .child(div().size(px(BLOB)).flex_none().child(mascot(&face, 0.)))
        .child(name);
    let rows = sessions.iter().filter(|_| open).map(|title| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .pl(px(BLOB + 6.))
            .py(px(2.))
            .text_style(TextStyle::Caption)
            .text_color(theme.text_muted)
            .child(
                icons::icon(kind_icon(Kind::Session))
                    .size(px(12.))
                    .text_color(theme.text_muted),
            )
            .child(*title)
    });
    div()
        .flex()
        .flex_col()
        .child(head)
        .children(rows)
        .into_any_element()
}
