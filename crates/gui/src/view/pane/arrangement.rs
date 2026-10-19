//! Drawing a space: the panes it arranges, the seams between them, and the
//! edges a drag can drop on.
//!
//! The tree is [`artifact::space::Node`] — see there for what the shape
//! means. This is only how it lands on the window.

use super::{Pane, Seat};
use crate::view::kit::menu;
use crate::{
    model::workspace::Showing,
    view::{
        component::divider,
        kit::new::{self, New},
        sidebar::{Dragged, Landed, Row},
        window::root::{Cydonia, NewSession},
    },
};
use artifact::kind::Kind;
use artifact::space::{Axis as Split, Member, Node, Side, Space};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, SharedString,
        StyleRefinement, Task, Window, div, prelude::*, px, relative,
    },
    theme::Theme,
    ui::{docking, icons, menu::Item},
};

/// The least of a split a pane may be squeezed to. A pane thinner than this
/// has nothing left to grab it by.
const MIN_SHARE: f64 = 0.08;

/// What a seam carries while it is dragged: the split it divides, by its path
/// from the root, and which of that split's children it sits after.
///
/// The path rather than a node id: the tree is rebuilt from the file on every
/// change, so nothing in it has an identity that outlives a drag.
#[derive(Clone, Debug)]
pub struct SeamDrag {
    pub path: Vec<usize>,
    pub at: usize,
}

impl Cydonia {
    /// The space the window is arranged by, taken whole: the tree is walked
    /// while the workspace is drawn from, so it is cloned out first.
    pub(crate) fn arrangement(&self, cx: &App) -> Option<Space> {
        self.workspace.read(cx).active_space().cloned()
    }

    /// The panes of the open space, or nothing where none is open and the
    /// window is showing one entry.
    pub(crate) fn panes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let space = self.arrangement(cx)?;
        // A zoomed pane stands over the rest, which keep their places
        // underneath — see [`Space::zoom`].
        if let Some(entry) = space.zoomed() {
            return Some(self.pane(&entry, window, cx));
        }
        Some(self.node(&space.tree, &mut Vec::new(), window, cx))
    }

    /// One node: a pane, or a split of them laid out along its axis.
    fn node(
        &mut self,
        node: &Node<Member>,
        path: &mut Vec<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            // `entry` is the pane's name, not what it is showing — the pane
            // resolves its own strip and which of it is in front.
            Node::Leaf { entry, .. } => self.pane(entry, window, cx),
            Node::Split { axis, children, .. } => {
                let axis = *axis;
                let theme = Theme::of(cx).clone();
                let floating = self.workspace.read(cx).settings.appearance.floating;
                let here = path.clone();
                let across = match axis {
                    Split::Horizontal => Axis::Horizontal,
                    Split::Vertical => Axis::Vertical,
                };
                let mut row = div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    .flex()
                    .map(|el| match axis {
                        Split::Horizontal => el.flex_row(),
                        Split::Vertical => el.flex_col(),
                    })
                    // On the split rather than on each seam: the bounds a drag
                    // is measured against are this split's, and a seam knows
                    // only its own.
                    .on_drag_move(cx.listener({
                        let here = here.clone();
                        move |this, event: &DragMoveEvent<SeamDrag>, _, cx| {
                            let drag = event.drag(cx);
                            if drag.path != here {
                                return;
                            }
                            let bounds = event.bounds;
                            let at = event.event.position;
                            let fraction = match axis {
                                Split::Horizontal => {
                                    (f32::from(at.x - bounds.left()) / f32::from(bounds.size.width))
                                        as f64
                                }
                                Split::Vertical => {
                                    (f32::from(at.y - bounds.top()) / f32::from(bounds.size.height))
                                        as f64
                                }
                            };
                            this.move_seam(&here, drag.at, fraction, cx);
                        }
                    }));
                for (ix, child) in children.iter().enumerate() {
                    path.push(ix);
                    let body = self.node(child, path, window, cx);
                    path.pop();
                    row = row.child(
                        div()
                            .min_w_0()
                            .min_h_0()
                            // Shrinks but does not grow: the shares are of the
                            // whole split and sum to it, and shrinking absorbs
                            // whatever rounding leaves over.
                            .flex_initial()
                            .flex()
                            .flex_col()
                            .map(|el| match axis {
                                Split::Horizontal => el.w(relative(child.ratio() as f32)).h_full(),
                                Split::Vertical => el.h(relative(child.ratio() as f32)).w_full(),
                            })
                            .child(body),
                    );
                }
                // The seams ride the boundary rather than sitting in flow, the
                // way the sidebar's does — a divider taking a column of its own
                // pushes the panes apart into a gap, where what is wanted is
                // one line between two panes that meet.
                let mut edge = 0.;
                for (ix, child) in children.iter().enumerate() {
                    edge += child.ratio() as f32;
                    if ix + 1 == children.len() {
                        break;
                    }
                    let (path, at) = (here.clone(), ix);
                    row = row.child(
                        divider::seam(&theme, across, floating)
                            .id(("pane-seam", seam_id(&here, at)))
                            .absolute()
                            .map(|el| match axis {
                                Split::Horizontal => el
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(edge))
                                    .ml(px(-divider::HIT / 2.)),
                                Split::Vertical => el
                                    .left_0()
                                    .right_0()
                                    .top(relative(edge))
                                    .mt(px(-divider::HIT / 2.)),
                            })
                            .on_drag(SeamDrag { path, at }, |_, _, _, cx| cx.new(|_| Empty)),
                    );
                }
                row.into_any_element()
            }
        }
    }

    /// One pane: the entries it holds, and whichever of them is in front.
    ///
    /// `entry` is the pane's *name* — the first of its strip, which is what
    /// the space keeps and what every drop and close here is aimed at. What
    /// the pane is showing is [`Workspace::front_of`], and the two are the same
    /// thing only for a pane holding one entry.
    fn pane(&mut self, entry: &Member, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let front = self
            .workspace
            .read(cx)
            .front_of(entry)
            .unwrap_or_else(|| entry.clone());
        let body = match self.pane_named(entry, cx) {
            Some(pane) => pane
                .cached(StyleRefinement::default().flex().flex_col().size_full())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };
        // By the tab in front: that is what a drop joins or divides.
        let body = self
            .dock
            .pane(front, px(crate::view::window::root::HEADER_HEIGHT), body);
        // Flat, the panes are one plane on the window's fill. Floating, each
        // pane draws its own card under its bar.
        let floating = self.workspace.read(cx).settings.appearance.floating;
        match floating {
            true => div(),
            false => bezel::ui::window::content(false, cx),
        }
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .child(body)
        .into_any_element()
    }

    /// The pane of the open space named `name`.
    pub(crate) fn pane_named(&self, name: &Member, cx: &App) -> Option<Entity<Pane>> {
        self.panes
            .iter()
            .find(|pane| pane.read(cx).name == *name)
            .cloned()
    }

    /// Make a pane for every pane of the open space and drop the rest, and
    /// hand each its tabs and where it stands in the window.
    pub(crate) fn sync_panes(&mut self, cx: &mut Context<Self>) {
        let Some(space) = self.arrangement(cx) else {
            self.panes.clear();
            return;
        };
        let first = space.entries().first().cloned();
        let last = match self.changes.is_none() {
            true => space.zoomed().or_else(|| top_right(&space.tree)),
            false => None,
        };
        let mut kept = Vec::new();
        for name in space.panes() {
            let stack = self.workspace.read(cx).stack_of(&name);
            // A pane is named by its first tab, and a close or a reorder can
            // hand the name on: the pane holding any of the strip is this one.
            // A close takes the old name out of the strip, so the tabs it held
            // are what still match it.
            let pane = match self
                .panes
                .iter()
                .position(|pane| stack.contains(&pane.read(cx).name))
                .or_else(|| {
                    self.panes.iter().position(|pane| {
                        pane.read(cx).tabs.iter().any(|tab| {
                            tab.read(cx)
                                .entry
                                .as_ref()
                                .is_some_and(|entry| stack.contains(entry))
                        })
                    })
                }) {
                Some(at) => self.panes.remove(at),
                None => {
                    let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
                    let name = name.clone();
                    cx.new(|cx| Pane::new(name, workspace, root, cx))
                }
            };
            let tabs: Vec<_> = stack
                .iter()
                .filter_map(|tab| {
                    self.leaves
                        .iter()
                        .find(|leaf| leaf.read(cx).entry.as_ref() == Some(tab))
                        .cloned()
                })
                .collect();
            let seat = Seat {
                first: first.as_ref() == Some(&name),
                last: last.as_ref() == Some(&name),
                sidebar_open: self.sidebar_open,
            };
            pane.update(cx, |pane, cx| {
                if pane.name != name || pane.tabs != tabs || pane.seat != seat {
                    pane.name = name.clone();
                    pane.tabs = tabs;
                    pane.seat = seat;
                    cx.notify();
                }
            });
            kept.push(pane);
        }
        self.panes = kept;
    }

    /// Bring `tab` to the front of the pane named `pane`.
    fn set_front(&mut self, pane: &Member, tab: &Member, cx: &mut Context<Self>) {
        self.sync_panes(cx);
        if let Some(pane) = self.pane_named(pane, cx) {
            let tab = tab.clone();
            pane.update(cx, |pane, cx| pane.set_front(tab, cx));
        }
    }

    /// Whether a pane draws the entry `row` names: the front tab of a pane of
    /// the open space, or the single pane.
    pub(crate) fn on_screen(&self, row: &Row, cx: &App) -> bool {
        let Row::Entry { project, showing } = row else {
            return false;
        };
        // The library stands over every pane.
        if self.library.is_some() {
            return false;
        }
        let workspace = self.workspace.read(cx);
        if let Some(space) = workspace.active_space() {
            let Some(at) = workspace.project_at(project) else {
                return false;
            };
            return space.tree.fronts().iter().any(|front| {
                workspace
                    .showing_of(front, cx)
                    .is_some_and(|(open, shown)| open == at && &shown == showing)
            });
        }
        let Some(open) = workspace
            .active_project()
            .filter(|open| open.location() == project)
        else {
            return false;
        };
        let Some(ix) = open.ix_of(showing) else {
            return false;
        };
        match (self.showing(cx), showing) {
            (Some(Kind::Article), Showing::Article(_)) => open.article == Some(ix),
            (Some(Kind::Board), Showing::Board(_)) => open.board == Some(ix),
            _ => false,
        }
    }

    /// Move `moving` to `to`'s place in their pane's strip. `false`, and
    /// nothing moves, where the two are not in one strip.
    ///
    /// A pane is named by its strip's first entry, so a move that changes the
    /// first renames the pane.
    pub(crate) fn reorder_tab(
        &mut self,
        moving: &Member,
        to: &Member,
        cx: &mut Context<Self>,
    ) -> bool {
        let stack = self.workspace.read(cx).stack_of(to);
        if moving == to || !stack.contains(moving) {
            return false;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.reorder_tab(moving, to, cx));
        let (Some(was), Some(now)) = (
            stack.first().cloned(),
            self.workspace.read(cx).stack_of(to).first().cloned(),
        ) else {
            return true;
        };
        if was != now
            && let Some(pane) = self.pane_named(&was, cx)
        {
            pane.update(cx, |pane, cx| {
                pane.name = now;
                cx.notify();
            });
        }
        cx.notify();
        true
    }

    /// Note a tab as the one most recently brought to the front, which is
    /// where a close falls back to — see [`Cydonia::tab_history`].
    fn remember_front(&mut self, tab: &Member) {
        self.tab_history.retain(|seen| seen != tab);
        self.tab_history.push(tab.clone());
    }

    /// Show one of a pane's tabs, and put the focus on it — a tab pressed is a
    /// pane entered, the same as a press anywhere else in one.
    pub(crate) fn show_tab(
        &mut self,
        pane: &Member,
        tab: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_front(pane, tab, cx);
        self.remember_front(tab);
        // Before the focus moves: a tab the space has gained since the last
        // frame has no leaf yet, and [`Cydonia::focus_pane`] moves nothing it
        // cannot find.
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(tab, window, cx);
        self.reveal_applied_match(cx);
        cx.notify();
    }

    /// What the single pane is showing, as a member.
    pub(crate) fn lone_member(&self, cx: &App) -> Option<Member> {
        self.workspace
            .read(cx)
            .active
            .zip(self.showing(cx))
            .and_then(|(project, pane)| self.member_showing(project, pane, cx))
    }

    /// The single entry's column, header included, as a pane a drop lands on:
    /// over the header it makes the two tabs, and on an edge the space that
    /// puts them side by side.
    pub(crate) fn lone_pane(&self, column: AnyElement, cx: &App) -> AnyElement {
        match self.lone_member(cx) {
            Some(on) => self
                .dock
                .pane(on, px(crate::view::window::root::HEADER_HEIGHT), column)
                .into_any_element(),
            None => column,
        }
    }

    /// The member a carried item names, once it has landed.
    fn dropped(&mut self, item: &Dragged, cx: &mut Context<Self>) -> Option<Landed> {
        match item {
            Dragged::Tab(member) => Some(Landed::Here(member.clone())),
            Dragged::Row(row) => row.landed(&self.workspace.clone(), cx),
        }
    }

    /// What a carried item is called, for the ghost that follows the pointer.
    pub(crate) fn label_of_dragged(&self, item: &Dragged, cx: &App) -> String {
        match item {
            Dragged::Row(row) => row.label(self.workspace.read(cx), cx),
            Dragged::Tab(member) => self
                .member_toolbar(member, cx)
                .map(|toolbar| toolbar.title)
                .unwrap_or_default(),
        }
    }

    /// A release on a pane: into its strip where it joins, beside it where
    /// it lands on an edge. Answers the pane the arrival is now in front of,
    /// and nothing for a session still being filed, which lands once it is.
    pub(crate) fn dock_drop(
        &mut self,
        event: &docking::Drop<Member, Dragged>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Member> {
        let target = &event.pane;
        let stack = self.workspace.read(cx).stack_of(target);
        if let Dragged::Tab(tab) = &event.item
            && stack.contains(tab)
            && (event.zone == docking::Zone::Join || stack.len() == 1)
        {
            return None;
        }
        match self.dropped(&event.item, cx)? {
            Landed::Here(arriving) => self.dock_member(target, arriving, event.zone, window, cx),
            Landed::Filing(filed) => {
                let (target, zone) = (target.clone(), event.zone);
                cx.spawn_in(window, async move |this, cx| {
                    let Some(arriving) = filed.await else {
                        return;
                    };
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.dock_member(&target, arriving, zone, window, cx)
                    });
                })
                .detach();
                None
            }
        }
    }

    /// Put `arriving` on `target` in `zone`.
    fn dock_member(
        &mut self,
        target: &Member,
        arriving: Member,
        zone: docking::Zone,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Member> {
        if arriving == *target {
            return None;
        }
        let side = match zone {
            docking::Zone::Join => {
                self.add_tab(target, arriving.clone(), window, cx);
                return Some(arriving);
            }
            docking::Zone::Left => Side::Left,
            docking::Zone::Right => Side::Right,
            docking::Zone::Top => Side::Above,
            docking::Zone::Bottom => Side::Below,
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.arrange(target, &arriving, side, cx)
        });
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(&arriving, window, cx);
        cx.notify();
        Some(arriving)
    }

    /// The member that names what a single pane is on.
    fn member_showing(&self, project: usize, pane: Kind, cx: &App) -> Option<Member> {
        let workspace = self.workspace.read(cx);
        let showing = workspace.projects.get(project)?.selected(pane)?;
        workspace.member_of(project, showing, cx)
    }

    /// What the `···` on a pane's bar offers: what can be done to the *pane*.
    ///
    /// Nothing about the entry it is on — no rename, no archive, no delete.
    /// Those act on a thing that has its own row in the sidebar and its own
    /// band when it is opened on its own, and `../desktop`'s rule for a `···`
    /// is that it carries only what has no affordance elsewhere. A delete one
    /// click from the moves would also be a delete nobody meant.
    ///
    /// Closing is not here either: it keeps the button on the bar.
    pub(crate) fn pane_rows(&self, entry: &Member, cx: &App) -> Vec<(Item, menu::Act)> {
        let zoomed = self
            .arrangement(cx)
            .and_then(|space| space.zoomed())
            .is_some_and(|at| at == *entry);

        let zoom = vec![menu::row::<Self>(
            match zoomed {
                true => Item::action("Restore").with_icon(icons::arrows::Shrink),
                false => Item::action("Expand").with_icon(icons::arrows::Expand),
            },
            {
                let on = entry.clone();
                move |this, _, cx| this.zoom_focused(&on, cx)
            },
        )];
        // Only the ways this pane can actually go: a move with nothing across
        // the seam is a row that does nothing, and a menu of those teaches
        // that the menu does nothing.
        let mut moves = Vec::new();
        for (side, label, icon) in [
            (Side::Left, "Move left", icons::arrows::ArrowLeft),
            (Side::Right, "Move right", icons::arrows::ArrowRight),
            (Side::Above, "Move up", icons::arrows::ArrowUp),
            (Side::Below, "Move down", icons::arrows::ArrowDown),
        ] {
            if self
                .workspace
                .read(cx)
                .neighbour_pane(entry, side)
                .is_none()
            {
                continue;
            }
            let on = entry.clone();
            moves.push(menu::row::<Self>(
                Item::action(label).with_icon(icon),
                move |this, window, cx| this.move_pane(&on, side, window, cx),
            ));
        }
        menu::sections([zoom, moves])
    }

    /// Exchange a pane with the one across the seam, and keep the focus on it
    /// — the pane moved, not the attention.
    fn move_pane(
        &mut self,
        entry: &Member,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved = self
            .workspace
            .update(cx, |workspace, cx| workspace.move_pane(entry, side, cx));
        if moved {
            self.sync_leaves(window, cx);
            self.focus_pane(entry, window, cx);
        }
    }

    /// Stand a pane over the others, or put it back.
    fn zoom_focused(&mut self, entry: &Member, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.zoom_pane(entry, cx));
    }

    /// Drop one pane from the arrangement.
    pub(crate) fn close_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // What the tab's own pane shows once it is gone — see
        // [`Pane::front_after`].
        let next = self
            .workspace
            .read(cx)
            .stack_of(entry)
            .first()
            .and_then(|name| self.pane_named(name, cx))
            .and_then(|pane| pane.read(cx).front_after(entry, cx));
        // Where the focus lands: where it is, unless it was on the tab
        // closing. Then on the tab its pane shows next, and for a pane's last
        // tab on the tab that was in front before it anywhere in the space,
        // or else the space's last tab.
        let entries = self
            .arrangement(cx)
            .map(|space| space.entries())
            .unwrap_or_default();
        let kept = self
            .leaf()
            .read(cx)
            .entry
            .clone()
            .filter(|focused| focused != entry)
            .or_else(|| next.clone())
            .or_else(|| {
                self.tab_history
                    .iter()
                    .rev()
                    .find(|tab| *tab != entry && entries.contains(tab))
                    .cloned()
            })
            .or_else(|| entries.iter().rev().find(|tab| *tab != entry).cloned());
        self.tab_history.retain(|tab| tab != entry);
        // The entry left when this close took the space with it. Without
        // putting the pane on its kind the window drops back to whatever the
        // single pane was last showing, which is not what was on screen.
        let alone = self
            .workspace
            .update(cx, |workspace, cx| workspace.close_pane(entry, cx));
        if let Some(next) = &next
            && let Some(pane) = self.workspace.read(cx).stack_of(next).first().cloned()
        {
            self.set_front(&pane, next, cx);
        }
        // The pane left alone is the one to keep, not the one just closed:
        // [`Cydonia::sync_leaves`] keeps whichever leaf is focused when it
        // finds no space, and the focus is still on the pane going away.
        if let Some((member, _)) = &alone
            && let Some(at) = self
                .leaves
                .iter()
                .position(|leaf| leaf.read(cx).entry.as_ref() == Some(member))
        {
            self.focused = at;
        }
        self.sync_leaves(window, cx);
        if let Some((_, showing)) = alone {
            self.show_pane(showing.kind(), cx);
        }
        if let Some(entry) = kept.or_else(|| self.leaf().read(cx).entry.clone()) {
            self.focused = usize::MAX;
            self.focus_pane(&entry, window, cx);
        }
        cx.notify();
    }

    /// Step the focus to the pane next along the arrangement.
    ///
    /// The order is the order the panes are laid out — left to right, and each
    /// column top to bottom — so this walks the window rather than jumping
    /// about it. `select-pane -LRUD` measured off the bounds would be truer to
    /// tmux; it needs each pane's frame, which nothing here keeps yet.
    pub(crate) fn step_pane(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        // Panes, not members: a pane holding three tabs is one stop on the
        // walk, and ⌃⇥ is what steps through what it holds.
        let panes = match self.arrangement(cx) {
            Some(space) => space.panes(),
            None => return,
        };
        if panes.len() < 2 {
            return;
        }
        let here = self.leaf().read(cx).entry.clone();
        let at = here
            .and_then(|front| {
                let stack = self.workspace.read(cx).stack_of(&front);
                let pane = stack.first()?;
                panes.iter().position(|named| named == pane)
            })
            .unwrap_or(0);
        let landing = (at as isize + step).rem_euclid(panes.len() as isize) as usize;
        let Some(pane) = panes.get(landing) else {
            return;
        };
        let front = self
            .workspace
            .read(cx)
            .front_of(pane)
            .unwrap_or_else(|| pane.clone());
        self.focused = usize::MAX;
        self.focus_pane(&front, window, cx);
    }

    /// Close the tab in front — and with it the pane, where it was the pane's
    /// last. The keyboard's half of the `×` on a tab.
    pub(crate) fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.leaf().read(cx).entry.clone() else {
            return;
        };
        self.close_pane(&entry, window, cx);
    }

    /// Take an entry that is going away — archived or deleted — out of the
    /// open space, and land on the top-left pane's front tab.
    pub(crate) fn put_away_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_pane(entry, window, cx);
        let Some(pane) = self
            .arrangement(cx)
            .and_then(|space| space.panes().first().cloned())
        else {
            return;
        };
        let front = self.workspace.read(cx).front_of(&pane).unwrap_or(pane);
        self.focused = usize::MAX;
        self.focus_pane(&front, window, cx);
    }

    /// Stand the pane in front over the others, or put it back.
    pub(crate) fn zoom_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.leaf().read(cx).entry.clone() else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.zoom_pane(&entry, cx);
        });
        cx.notify();
    }

    /// Open a space: the window is arranged by it until another entry is
    /// opened on its own.
    pub(crate) fn open_space(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.library = None;
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_space(ix, cx);
        });
        self.sync_leaves(window, cx);
        // The focus lands on the tab in front of the pane the space last had
        // focused, or else of the first pane it lays out, at its top left.
        let front = self.workspace.read(cx).active_space().and_then(|space| {
            let pane = space.focused().or_else(|| space.panes().first().cloned())?;
            space.front_of(&pane)
        });
        if let Some(entry) = front {
            self.focused = usize::MAX;
            self.focus_pane(&entry, window, cx);
        }
        cx.notify();
    }

    /// Put `arriving` in `pane`'s strip as its front tab, and focus it.
    pub(crate) fn add_tab(
        &mut self,
        pane: &Member,
        arriving: Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.stack_pane(pane, &arriving, cx)
        });
        // The pane's name can have changed under the move — a tab dragged out
        // of a strip leaves the one behind it holding the pane — so the strip
        // is read back rather than assumed.
        let stack = self.workspace.read(cx).stack_of(&arriving);
        if let Some(pane) = stack.first().cloned() {
            self.set_front(&pane, &arriving, cx);
            self.remember_front(&arriving);
        }
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(&arriving, window, cx);
        cx.notify();
    }

    /// What the `+` in a pane's bar starts: the sidebar's `+` for the
    /// project of the pane's front tab, landing as a tab of the pane.
    pub(crate) fn pane_add_rows(
        &self,
        pane: &Member,
        project: usize,
        cx: &App,
    ) -> Vec<(Item, menu::Act)> {
        new::rows(new::offered(self.workspace.read(cx)), |kind| {
            let pane = pane.clone();
            Box::new(move |this: &mut Cydonia, window, cx| {
                this.new_in_pane(&pane, project, kind, window, cx)
            })
        })
    }

    /// Make an entry in `project` and open it as the front tab of `pane`.
    fn new_in_pane(
        &mut self,
        pane: &Member,
        project: usize,
        kind: New,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        if let New::Board = kind {
            return self.ask_new_board_into(project, Some(pane.clone()), window, cx);
        }
        // No agent to open one on: the session pane says so and offers the
        // install.
        if let New::Session(None) = kind
            && self.workspace.read(cx).preferred_agent(cx).is_none()
        {
            return self.new_session_action(&NewSession, window, cx);
        }
        let member: Task<Option<Member>> = self.workspace.update(cx, |workspace, cx| {
            workspace.select_project(project, cx);
            match kind {
                New::Session(at) => {
                    let Some(id) = workspace
                        .agent_at(at, cx)
                        .and_then(|agent| workspace.new_session(agent, None, cx))
                    else {
                        return Task::ready(None);
                    };
                    // A space names its members by file, and a new session
                    // has none until its first turn.
                    let retained = workspace.retain_session(id, cx);
                    cx.spawn(async move |workspace, cx| {
                        retained.await?;
                        workspace
                            .update(cx, |workspace, cx| {
                                workspace.member_of(project, Showing::Session(id), cx)
                            })
                            .ok()
                            .flatten()
                    })
                }
                New::Article => {
                    let made = workspace.create_article(cx);
                    cx.spawn(async move |workspace, cx| {
                        let (project, ix) = made.await?;
                        workspace
                            .update(cx, |workspace, cx| {
                                let id = workspace
                                    .projects
                                    .get(project)?
                                    .articles
                                    .get(ix)?
                                    .id
                                    .clone();
                                workspace.member_of(project, Showing::Article(id), cx)
                            })
                            .ok()
                            .flatten()
                    })
                }
                New::Table => {
                    let made = workspace.create_table(cx);
                    cx.spawn(async move |workspace, cx| {
                        let (project, ix) = made.await?;
                        workspace
                            .update(cx, |workspace, cx| {
                                let key =
                                    workspace.projects.get(project)?.tables.get(ix)?.key.clone();
                                workspace.member_of(project, Showing::Table(key), cx)
                            })
                            .ok()
                            .flatten()
                    })
                }
                New::Board => Task::ready(None),
            }
        });
        let pane = pane.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Some(member) = member.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.add_tab(&pane, member, window, cx)
            });
        })
        .detach();
    }

    /// Put the seam after `at` where the pointer left it.
    fn move_seam(&mut self, path: &[usize], at: usize, fraction: f64, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            let Some(space) = workspace.active_space_mut() else {
                return;
            };
            let Some(split) = space.tree.at_path_mut(path) else {
                return;
            };
            if split.resize(at, fraction, MIN_SHARE) {
                cx.notify();
            }
        });
    }
}

/// A member as something that can be an element id: an entry by which
/// project and which of its things — two projects can hold the same id, so
/// the path is part of it — or an app by its id.
pub(crate) fn key_of(member: &Member) -> SharedString {
    SharedString::from(match member {
        Member::Entry(entry) => format!("{}::{:?}::{}", entry.project, entry.kind, entry.id),
        Member::App { app } => format!("app::{app}"),
    })
}

/// A seam's place in the window, as something that can be an element id.
fn seam_id(path: &[usize], at: usize) -> usize {
    path.iter()
        .fold(1usize, |id, step| id.wrapping_mul(31).wrapping_add(*step))
        .wrapping_mul(31)
        .wrapping_add(at)
}

#[cfg(test)]
#[path = "../../../tests/unit/open_entries.rs"]
mod open_entry_tests;

/// The pane at a tree's top right: the last of a row, the first of a column.
fn top_right(node: &Node<Member>) -> Option<Member> {
    match node {
        Node::Leaf { entry, .. } => Some(entry.clone()),
        Node::Split { axis, children, .. } => match axis {
            Split::Horizontal => children.last(),
            Split::Vertical => children.first(),
        }
        .and_then(top_right),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/pane_footer.rs"]
mod pane_footer_tests;
