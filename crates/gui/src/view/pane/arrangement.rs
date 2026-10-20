//! Drawing a space: the panes it arranges, the seams between them, and the
//! edges a drag can drop on.
//!
//! The tree is [`artifact::space::Node`] — see there for what the shape
//! means. This is only how it lands on the window.

use super::Pane;
use crate::model::workspace::Slot;
use crate::view::kit::menu;
use crate::{
    model::workspace::Showing,
    view::{
        component::divider,
        kit::new::{self, New},
        sidebar::{Dragged, Row},
        window::root::{Cydonia, NewSession},
    },
};
use artifact::space::{Axis as Split, Member, Node, Side, Space};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, SharedString,
        StyleRefinement, Window, div, prelude::*, px, relative,
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
    /// What the window is arranged by, taken whole — see
    /// [`crate::model::workspace::Workspace::arranged`]. The tree is walked
    /// while the workspace is drawn from, so it is cloned out first.
    pub(crate) fn arrangement(&self, cx: &App) -> Option<Space> {
        self.workspace.read(cx).arranged()
    }

    /// The panes the window is arranged in, or nothing where no tab is open.
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
                        pane.read(cx)
                            .tabs
                            .iter()
                            .any(|tab| stack.contains(&tab.read(cx).entry))
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
                        .find(|leaf| leaf.read(cx).entry == *tab)
                        .cloned()
                })
                .collect();
            pane.update(cx, |pane, cx| {
                if pane.name != name || pane.tabs != tabs {
                    pane.name = name.clone();
                    pane.tabs = tabs;
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

    /// Whether a pane draws the entry `row` names: the front tab of a pane.
    pub(crate) fn on_screen(&self, row: &Row, cx: &App) -> bool {
        let Row::Entry { project, showing } = row else {
            return false;
        };
        // The library stands over every pane.
        if self.library.is_some() {
            return false;
        }
        let workspace = self.workspace.read(cx);
        let (Some(space), Some(at)) = (workspace.arranged(), workspace.project_at(project)) else {
            return false;
        };
        space.tree.fronts().iter().any(|front| {
            workspace
                .showing_of(front, cx)
                .is_some_and(|(open, shown)| open == at && &shown == showing)
        })
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

    /// The member a carried item names, once it has landed.
    fn dropped(&mut self, item: &Dragged, cx: &mut Context<Self>) -> Option<Member> {
        match item {
            Dragged::Tab(slot) => slot.entry().cloned(),
            Dragged::Row(row) => row.landed(&self.workspace.clone(), cx),
        }
    }

    /// What a carried item is called, for the ghost that follows the pointer.
    pub(crate) fn label_of_dragged(&self, item: &Dragged, cx: &App) -> String {
        match item {
            Dragged::Row(row) => row.label(self.workspace.read(cx), cx),
            Dragged::Tab(Slot::Entry(member)) => self
                .member_toolbar(member, cx)
                .map(|toolbar| toolbar.title)
                .unwrap_or_default(),
            Dragged::Tab(Slot::Space { space }) => {
                let workspace = self.workspace.read(cx);
                workspace
                    .space_ix(space)
                    .and_then(|ix| workspace.spaces.get(ix))
                    .map(|space| space.label().to_owned())
                    .unwrap_or_default()
            }
        }
    }

    /// A release on a pane: beside it where it lands on an edge, and as a tab
    /// of the strip anywhere else. Answers the entry the focus is now on.
    pub(crate) fn dock_drop(
        &mut self,
        event: &docking::Drop<Member, Dragged>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Member> {
        let target = &event.pane;
        let stack = self.workspace.read(cx).stack_of(target);
        if let Dragged::Tab(Slot::Entry(tab)) = &event.item
            && stack.contains(tab)
            && (event.zone == docking::Zone::Join || stack.len() == 1)
        {
            return None;
        }
        let arriving = self.dropped(&event.item, cx)?;
        if arriving == *target {
            return None;
        }
        let side = match event.zone {
            docking::Zone::Join => {
                self.join_strip(arriving.clone(), window, cx);
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

    /// What can be done to the pane of a space `entry` is on: zoom it, move
    /// it across a seam, close it. Nothing for an entry no space in front
    /// holds beside another — see [`crate::view::pane::title`].
    pub(crate) fn pane_rows(&self, entry: &Member, cx: &App) -> Vec<(Item, menu::Act)> {
        let several = self
            .workspace
            .read(cx)
            .active_space()
            .is_some_and(|space| space.contains(entry) && space.leaves() > 1);
        if !several {
            return Vec::new();
        }
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
        let close = vec![menu::row::<Self>(
            Item::action("Close pane").with_icon(icons::notifications::X),
            {
                let on = entry.clone();
                move |this, window, cx| this.close_pane(&on, window, cx)
            },
        )];
        menu::sections([zoom, moves, close])
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
            .focused_entry(cx)
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
        // The entry left when this close took the space with it, which opened
        // in the window's strip — see [`Workspace::close_pane`].
        let alone = self
            .workspace
            .update(cx, |workspace, cx| workspace.close_pane(entry, cx));
        if let Some(next) = &next
            && let Some(pane) = self.workspace.read(cx).stack_of(next).first().cloned()
        {
            self.set_front(&pane, next, cx);
        }
        self.sync_leaves(window, cx);
        let landing = alone
            .map(|(member, _)| member)
            .or(kept)
            .filter(|member| {
                self.leaves
                    .iter()
                    .any(|leaf| leaf.read(cx).entry == *member)
            })
            .or_else(|| self.focused_entry(cx));
        if let Some(entry) = landing {
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
        let here = self.focused_entry(cx);
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
        let Some(entry) = self.focused_entry(cx) else {
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
        let Some(entry) = self.focused_entry(cx) else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.zoom_pane(&entry, cx);
        });
        cx.notify();
    }

    /// Open a space: the window is arranged by it until an entry no space
    /// holds is opened.
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

    /// Open `arriving` as a tab of the strip, in front, and focus it.
    pub(crate) fn join_strip(
        &mut self,
        arriving: Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.library = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.join_strip(&arriving, cx));
        self.remember_front(&arriving);
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(&arriving, window, cx);
        cx.notify();
    }

    /// What the strip's `+` starts: the sidebar's `+` for `project`,
    /// landing as a tab of its own.
    pub(crate) fn new_tab_rows(&self, project: usize, cx: &App) -> Vec<(Item, menu::Act)> {
        new::rows(new::offered(self.workspace.read(cx)), |kind| {
            Box::new(move |this: &mut Cydonia, window, cx| {
                this.new_in_tab(project, kind, window, cx)
            })
        })
    }

    /// Make an entry in `project` and open it as a tab of its own.
    fn new_in_tab(
        &mut self,
        project: usize,
        kind: New,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        if let New::Board = kind {
            return self.ask_new_board(project, window, cx);
        }
        // No agent to open one on: the session pane says so and offers the
        // install.
        if let New::Session(None) = kind
            && self.workspace.read(cx).preferred_agent(cx).is_none()
        {
            return self.new_session_action(&NewSession, window, cx);
        }
        let member = self.workspace.update(cx, |workspace, cx| {
            workspace.select_project(project, cx);
            let showing = match kind {
                New::Session(at) => {
                    let agent = workspace.agent_at(at, cx)?;
                    let id = workspace.new_session(agent, None, cx)?;
                    workspace.mint_record(id, cx)?;
                    Showing::Session(id)
                }
                New::Article => {
                    let ix = workspace.create_article()?;
                    Showing::Article(
                        workspace
                            .projects
                            .get(project)?
                            .articles
                            .get(ix)?
                            .id
                            .clone(),
                    )
                }
                New::Table => {
                    let ix = workspace.create_table()?;
                    Showing::Table(workspace.projects.get(project)?.tables.get(ix)?.key.clone())
                }
                New::Board => return None,
            };
            workspace.member_of(project, showing, cx)
        });
        if let Some(member) = member {
            self.join_strip(member, window, cx);
        }
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
        Member::Entry(entry) => format!(
            "{}::{:?}::{}",
            entry.project.display(),
            entry.kind,
            entry.id
        ),
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

#[cfg(test)]
#[path = "../../../tests/unit/pane_footer.rs"]
mod pane_footer_tests;
