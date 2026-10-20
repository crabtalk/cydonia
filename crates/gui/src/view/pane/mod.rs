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
/// row. A leaf a space stacks several entries in shows its front.
pub(crate) struct Pane {
    /// The pane's name: the first entry of its leaf, which is what the space
    /// keeps and what every drop and close is aimed at.
    pub(crate) name: Member,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// The tab of each entry of the leaf, in the leaf's order.
    pub(crate) tabs: Vec<Entity<Tab>>,
    /// The tabs brought to the front since the pane was made, most recent
    /// last — what [`Self::front_after`] reads. Runtime only; the tab in
    /// front is the space's.
    fronts: Vec<Member>,
}

impl Pane {
    /// Whether one of this pane's tabs holds the window's focus.
    pub(crate) fn focused(&self, cx: &App) -> bool {
        self.tabs.iter().any(|tab| tab.read(cx).focused)
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
            tabs: Vec::new(),
            fronts: Vec::new(),
        }
    }

    /// The pane's strip, as the space orders it.
    pub(crate) fn stack(&self, cx: &App) -> Vec<Member> {
        self.workspace.read(cx).stack_of(&self.name)
    }

    /// The entry in front.
    pub(crate) fn front(&self, cx: &App) -> Member {
        self.workspace
            .read(cx)
            .front_of(&self.name)
            .unwrap_or_else(|| self.name.clone())
    }

    pub(crate) fn set_front(&mut self, front: Member, cx: &mut Context<Self>) {
        let stack = self.stack(cx);
        self.fronts
            .retain(|seen| *seen != front && stack.contains(seen));
        self.fronts.push(front.clone());
        self.workspace
            .update(cx, |workspace, cx| workspace.set_front(&front, cx));
        cx.notify();
    }

    /// The tab in front once `gone` leaves the strip: the one in front now
    /// where that is not `gone`; else the one in front before it, or else its
    /// neighbour — to its left, to its right for the first. `None` where
    /// `gone` is the strip's last tab, or not in it.
    pub(crate) fn front_after(&self, gone: &Member, cx: &App) -> Option<Member> {
        let stack = self.stack(cx);
        let at = stack.iter().position(|tab| tab == gone)?;
        let front = self.front(cx);
        if front != *gone {
            return Some(front);
        }
        self.fronts
            .iter()
            .rev()
            .find(|tab| *tab != gone && stack.contains(tab))
            .or_else(|| match at {
                0 => stack.get(1),
                at => stack.get(at - 1),
            })
            .cloned()
    }

    /// The tab in front.
    pub(crate) fn front_tab(&self, cx: &App) -> Option<Entity<Tab>> {
        let front = self.front(cx);
        self.tabs
            .iter()
            .find(|tab| tab.read(cx).entry == front)
            .cloned()
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let key = arrangement::key_of(&self.name);
        let front = self.front(cx);
        let tab = self.front_tab(cx);
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
            // the focus — see [`Tab::focus`]. The front tab's handle: the
            // body, the composer and [`Cydonia::focus_pane`] all answer for
            // the tab in front.
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
