//! The latest release's notes. Rewritten, [`VERSION`] included, for a release
//! that has something to show; a release that leaves this file alone raises
//! nothing on launch.

use crate::{model::workspace::Workspace, view::kit::icon::kind_icon};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        AnyElement, Context, Div, Entity, Subscription, Task, Window, div, prelude::*, px, relative,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tabs::{self, Label, State, Strip, Track},
        widgets::Controls,
    },
};
use std::time::Duration;

/// The release these notes are for.
pub(crate) const VERSION: &str = "0.2.4";

/// Each page's line under the title, in page order.
pub(super) const HEADLINES: [&str; 2] = ["One strip of tabs per window.", "Spaces are tabs too."];

/// The tabs the first page's window holds, by their place in [`Body::strip`].
const TABS: [(Kind, &str); 3] = [
    (Kind::Session, "Session"),
    (Kind::Board, "Board"),
    (Kind::Article, "Article"),
];

/// How long the strip holds still between two steps of its loop.
const STEP: Duration = Duration::from_millis(1100);

/// The window in miniature: how tall it stands, the lights' dots, how far a
/// floating card sits in from its edges, and how wide its sidebar is.
const WINDOW_HEIGHT: f32 = 140.;
const LIGHT: f32 = 8.;
const INSET: f32 = 4.;
const SIDEBAR: f32 = 110.;

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

/// What follows a page's headline: a window in miniature and the switch it is
/// drawn by. The first page's strip of tabs steps through its loop over the
/// floating or the flat layout; the second shows a space as a tab, or listed
/// in the sidebar.
pub(super) struct Body {
    workspace: Entity<Workspace>,
    /// Which of [`HEADLINES`]' pages is shown.
    pub(super) page: usize,
    strip: Strip<usize>,
    reorder: tabs::Reorder<usize>,
    /// The second page's strip: the space's tab in front of a session's.
    spaces: tabs::Reorder<usize>,
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
            page: 0,
            strip: (0..TABS.len()).collect(),
            reorder: tabs::Reorder::new(),
            spaces: tabs::Reorder::new(),
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
        if self.page == 0 {
            cx.notify();
        }
    }

    /// The first page's strip: the tabs of [`TABS`] in [`Self::strip`]'s order.
    fn tabs(&self, floating: bool, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let tabs = self.strip.tabs().iter().map(|&tab| {
            let (kind, name) = TABS[tab];
            let label = Label::new(name).with_icon(kind_icon(kind));
            (
                tab,
                tabs::tab(
                    theme,
                    format!("whats-new-{name}"),
                    label,
                    state(&self.strip, tab),
                ),
            )
        });
        self.reorder
            .bar("whats-new-tabs", &self.strip, tabs)
            .on_track(track(floating))
            .on_reorder(cx.listener(|this, moved: &tabs::Move, _, cx| {
                this.strip.reorder(moved.from, moved.to);
                cx.notify();
            }))
            .into_any_element()
    }

    /// The second page's strip: a space's tab in front of a session's.
    fn space_tabs(&self, floating: bool, theme: &Theme) -> AnyElement {
        let strip: Strip<usize> = (0..2).collect();
        let tabs = [
            Label::new("Space").with_icon(icons::layout::LayoutDashboard),
            Label::new("Session").with_icon(kind_icon(Kind::Session)),
        ]
        .into_iter()
        .enumerate()
        .map(|(tab, label)| {
            let key = format!("whats-new-space-{tab}");
            (tab, tabs::tab(theme, key, label, state(&strip, tab)))
        });
        self.spaces
            .bar("whats-new-space-tabs", &strip, tabs)
            .on_track(track(floating))
            .into_any_element()
    }

    /// The first page: the strip's loop, and the floating switch.
    fn strip_page(&self, floating: bool, theme: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let workspace = self.workspace.clone();
        let window = window(
            floating,
            theme,
            Some(self.tabs(floating, theme, cx)),
            None,
            vec![div().flex_1().into_any_element()],
        );
        let switch = theme
            .toggle(floating)
            .id("whats-new-floating")
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                workspace.update(cx, |workspace, cx| workspace.set_floating(!floating, cx))
            });
        vec![
            window.into_any_element(),
            row([
                side("Flat", theme),
                switch.into_any_element(),
                side("Floating", theme),
            ])
            .into_any_element(),
        ]
    }

    /// The second page: a space as a tab of the strip, or listed in the
    /// sidebar with its entries under it, and the switch between the two.
    fn spaces_page(&self, floating: bool, theme: &Theme, cx: &Context<Self>) -> Vec<AnyElement> {
        let listed = self
            .workspace
            .read(cx)
            .settings
            .appearance
            .spaces_in_sidebar;
        let panes = [(Kind::Board, "Board"), (Kind::Article, "Article")]
            .map(|(kind, name)| pane(kind, name, theme).into_any_element())
            .into_iter()
            .collect();
        let window = match listed {
            true => window(floating, theme, None, Some(sidebar(theme)), panes),
            false => window(
                floating,
                theme,
                Some(self.space_tabs(floating, theme)),
                None,
                panes,
            ),
        };
        let workspace = self.workspace.clone();
        let switch = theme
            .toggle(listed)
            .id("whats-new-spaces")
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.set_spaces_in_sidebar(!listed, cx)
                })
            });
        vec![
            window.into_any_element(),
            row([side("Spaces in sidebar", theme), switch.into_any_element()]).into_any_element(),
        ]
    }
}

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        let page = match self.page {
            0 => self.strip_page(floating, &theme, cx),
            _ => self.spaces_page(floating, &theme, cx),
        };
        div().py_2().flex().flex_col().gap(px(14.)).children(page)
    }
}

/// How a strip's tab `tab` stands: the one in front focused, the rest resting.
fn state(strip: &Strip<usize>, tab: usize) -> State {
    match strip.active() == Some(&tab) {
        true => State::Focused,
        false => State::Resting,
    }
}

/// What a strip's tabs stand on in the floating or the flat layout, as the
/// window's own strip has them.
fn track(floating: bool) -> Track {
    match floating {
        true => Track::Bare,
        false => Track::Flat,
    }
}

/// A window in miniature: its lights and `strip` along the top, `sidebar`
/// down its left, and `panes` side by side, on a card when floating and
/// a plain body when flat.
fn window(
    floating: bool,
    theme: &Theme,
    strip: Option<AnyElement>,
    sidebar: Option<AnyElement>,
    panes: Vec<AnyElement>,
) -> Div {
    let lights = div()
        .flex_none()
        .flex()
        .gap(px(LIGHT / 2.))
        .children((0..3).map(|_| div().size(px(LIGHT)).rounded_full().bg(theme.text_faint)));
    let top = div()
        .flex_none()
        .min_h(px(tabs::TRACK_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .py(px(INSET))
        .child(lights)
        .children(strip.map(|strip| div().flex_1().min_w_0().flex().child(strip)));
    let body = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_row()
        .overflow_hidden()
        .bg(bezel::ui::window::content_fill(floating, theme))
        .children(panes.into_iter().enumerate().map(|(at, pane)| {
            div()
                .flex_1()
                .when(at > 0, |el| el.border_l_1().border_color(theme.border))
                .child(pane)
        }));
    let body = match floating {
        true => body
            .mr(px(INSET))
            .mb(px(INSET))
            .when(sidebar.is_none(), |body| body.ml(px(INSET)))
            .rounded(px(INSET * 2.))
            .border_1()
            .border_color(theme.border),
        false => body.border_t_1().border_color(theme.border),
    };
    let under = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_row()
        .children(sidebar)
        .child(body);
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
        .child(under)
}

/// A pane of a space in miniature: its title row, its entry's mark and name.
fn pane(kind: Kind, name: &'static str, theme: &Theme) -> Div {
    div()
        .p(px(8.))
        .child(line(kind_icon(kind), name, 0., theme))
}

/// The sidebar in miniature: a space with its two entries under it, then a
/// session on its own.
fn sidebar(theme: &Theme) -> AnyElement {
    div()
        .flex_none()
        .w(px(SIDEBAR))
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(10.))
        .py(px(6.))
        .child(line(
            icons::layout::LayoutDashboard.into(),
            "Space",
            0.,
            theme,
        ))
        .child(line(kind_icon(Kind::Board), "Board", 12., theme))
        .child(line(kind_icon(Kind::Article), "Article", 12., theme))
        .child(line(kind_icon(Kind::Session), "Session", 0., theme))
        .into_any_element()
}

/// One line of a miniature: a mark and a name, `indent` in from its start.
fn line(icon: icons::Icon, name: &'static str, indent: f32, theme: &Theme) -> Div {
    div()
        .pl(px(indent))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .text_style(TextStyle::Caption)
        .text_color(theme.text_muted)
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

/// A page's switch row, centred under its window.
fn row(children: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .children(children)
}
