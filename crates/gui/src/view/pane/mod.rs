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

/// One pane of what the window lays out: the entry it shows under its title
/// row.
pub(crate) struct Pane {
    /// The pane's name: the entry of its leaf, which is what the space keeps
    /// and what every drop and close is aimed at.
    pub(crate) name: Member,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// The tab on the entry, once the window has made it.
    pub(crate) tab: Option<Entity<Tab>>,
}

impl Pane {
    /// Whether this pane's tab holds the window's focus.
    pub(crate) fn focused(&self, cx: &App) -> bool {
        self.tab.as_ref().is_some_and(|tab| tab.read(cx).focused)
    }

    pub(crate) fn new(
        name: Member,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            name,
            workspace,
            root,
            tab: None,
        }
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let key = arrangement::key_of(&self.name);
        let front = self.name.clone();
        let tab = self.tab.clone();
        let focus = tab.as_ref().map(|tab| tab.read(cx).focus.clone());
        let body = match tab {
            Some(tab) => tab
                .cached(StyleRefinement::default().flex().flex_col().size_full())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };
        let root = self.root.clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
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
            .group("pane")
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
                        .when(self.focused(cx), |card| card.border_color(strong)),
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
