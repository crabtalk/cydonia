//! Panes: how what the window lays out is arranged, and what every pane carries.

pub mod arrangement;
pub mod drawer;
pub mod find;
pub mod footer;
pub mod header;
pub mod items;
pub mod tab;
pub mod title;

pub(crate) use tab::{Tab, TabMenu};

use crate::{model::workspace::Workspace, view::window::root::Cydonia};
use artifact::space::Member;
use bezel::{
    gpui::{
        App, Context, Entity, MouseButton, SharedString, StyleRefinement, WeakEntity, Window, div,
        prelude::*,
    },
    theme::Theme,
};

/// The key context of a pane holding more than one entry.
pub(crate) const STACK_CONTEXT: &str = "Stack";

/// Where a pane stands against the window's top edge, while the window draws
/// no top row of its own and the title rows along that edge are it — see
/// [`Cydonia::sync_edges`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edge {
    /// Its title row is part of the window's top row: the window is dragged
    /// by it, and a floating card has no margin above it.
    pub top: bool,
    /// It is at the window's top left with the sidebar folded: its title row
    /// clears the lights and holds the sidebar's fold.
    pub lights: bool,
    /// It is at the window's top right with the right panel down: its title
    /// row ends with the panel's toggle.
    pub toggle: bool,
}

/// One pane of what the window lays out: the front of its stack, under its
/// title row.
pub(crate) struct Pane {
    /// The entry in front of its stack, which every drop and close is aimed
    /// at.
    pub(crate) entry: Member,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// The tab on the entry, once the window has made it.
    pub(crate) tab: Option<Entity<Tab>>,
    /// Its title row is part of the window's top row — see [`Edge::top`].
    pub(crate) top: bool,
}

impl Pane {
    /// Whether this pane's tab holds the window's focus.
    pub(crate) fn focused(&self, cx: &App) -> bool {
        self.tab.as_ref().is_some_and(|tab| tab.read(cx).focused)
    }

    pub(crate) fn new(
        entry: Member,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            entry,
            workspace,
            root,
            tab: None,
            top: false,
        }
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let key = arrangement::key_of(&self.entry);
        let front = self.entry.clone();
        let tab = self.tab.clone();
        let focus = tab.as_ref().map(|tab| tab.read(cx).focus.clone());
        let body = match tab {
            Some(tab) => tab
                .cached(StyleRefinement::default().flex().flex_col().size_full())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };
        let root = self.root.clone();
        let workspace = self.workspace.read(cx);
        let floating = workspace.settings.appearance.floating;
        let stacked = workspace.stack_of(&self.entry).len() > 1;
        // The focused pane's card has the stronger edge.
        let strong = theme.border_strong;
        div()
            .id(SharedString::from(format!("pane-{key}")))
            // With the context but without this, a pane claims chords that
            // never reach it: an action runs through the focused element's
            // ancestors, and a pane showing a board holds nothing that takes
            // the focus — see [`Tab::focus`]. The tab's handle: the body, the
            // composer and [`Cydonia::focus_pane`] all answer for it.
            .when_some(focus, |pane, focus| pane.track_focus(&focus))
            .when(stacked, |pane| pane.key_context(STACK_CONTEXT))
            .size_full()
            .min_w_0()
            .min_h_0()
            // A flex column, not just a box: every pane draws itself with
            // `flex_1`, which fills nothing at all outside one.
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            // No fill: flat, the panes sit on the detail column's one surface,
            // and floating, the card fills itself.
            //
            // Pressing anywhere in a pane is how the focus moves to it, the
            // same way a click into the sidebar selects a row.
            .on_mouse_down(MouseButton::Left, {
                let (root, on) = (root.clone(), front.clone());
                move |_, window, cx| {
                    root.update(cx, |root, cx| root.focus_pane(&on, window, cx))
                        .ok();
                }
            })
            // Before anything inside takes the press: a link pressed in this
            // pane opens in its drawer.
            .capture_any_mouse_down({
                let on = front.clone();
                move |_, _, cx| {
                    root.update(cx, |root, _| root.pressed_pane = Some(on.clone()))
                        .ok();
                }
            })
            .child(
                // A flex item rather than full size: the card's margin is what
                // leaves the gap, and a full-size card overflows by it.
                match floating {
                    true => bezel::ui::window::content(true, cx)
                        .when(self.focused(cx), |card| card.border_color(strong))
                        .when(self.top, |card| card.mt_0()),
                    false => div(),
                }
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(body),
            )
    }
}
