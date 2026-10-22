//! Drawing a space: the panes it arranges, the seams between them, and the
//! edges a drag can drop on.
//!
//! The tree is [`artifact::space::Node`] — see there for what the shape
//! means. This is only how it lands on the window.

use crate::{
    model::workspace::{Member, Slot, Space},
    view::{
        component::divider,
        kit::{icon, menu},
        pane::{Edge, Pane},
        sidebar::{Dragged, Row},
        window::root::{self, Cydonia},
    },
};
use artifact::space::{Axis as Split, Node, Side};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Bounds, Context, DragMoveEvent, Empty, Entity, Pixels, SharedString,
        StyleRefinement, Window, canvas, div, prelude::*, px, relative,
    },
    motion::{self, AppExt as _, LAYOUT},
    theme::Theme,
    ui::{docking, icons, menu::Item},
};
use std::{borrow::Cow, collections::HashMap};
use web_time::Instant;

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

/// A pane gliding between its place in the space and the whole of it.
pub(crate) struct ZoomGlide {
    entry: Member,
    /// Its place in the space, from the space's top left.
    from: Bounds<Pixels>,
    /// Toward the whole of the space, or back to its place.
    expanding: bool,
    since: Instant,
}

/// Where the panes of the space in front and the space itself were last laid
/// out, in the window's coordinates. A zoomed space draws one pane, so its
/// others keep the place they had before it zoomed.
#[derive(Default)]
pub(crate) struct Places {
    panes: HashMap<Member, Bounds<Pixels>>,
    space: Option<Bounds<Pixels>>,
}

impl Cydonia {
    /// What the window is arranged by — see
    /// [`crate::model::workspace::Workspace::arranged`].
    pub(crate) fn arrangement<'a>(&self, cx: &'a App) -> Option<Cow<'a, Space>> {
        self.workspace.read(cx).arranged()
    }

    /// The panes the window is arranged in, or nothing where no tab is open.
    pub(crate) fn panes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // The tree is walked while the window is drawn into.
        let space = self.arrangement(cx)?.into_owned();
        let places = self.pane_places.clone();
        let measured = canvas(
            move |bounds, _, _| places.borrow_mut().space = Some(bounds),
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let total = LAYOUT.total().mul_f32(motion::speed_scale()).as_secs_f32();
        let glide = self.zoom_glide.as_ref().and_then(|glide| {
            let raw = glide.since.elapsed().as_secs_f32() / total.max(f32::EPSILON);
            (raw < 1.0 && space.contains(&glide.entry)).then(|| {
                let t = LAYOUT.progress(raw);
                let t = if glide.expanding { t } else { 1.0 - t };
                (glide.entry.clone(), glide.from, t)
            })
        });
        let body = match (glide, space.zoomed()) {
            // The pane on its way, over the rest in their places with its own
            // left empty.
            (Some((entry, from, t)), _) => {
                window.request_animation_frame();
                let at =
                    |from: Pixels, to: Pixels| px(motion::lerp(f32::from(from), f32::from(to), t));
                let whole = self
                    .pane_places
                    .borrow()
                    .space
                    .map(|space| space.size)
                    .unwrap_or(from.size);
                let tree = self.node(&space.tree, &mut Vec::new(), Some(&entry), window, cx);
                // Opaque under the moving pane, whose own fill lets what is
                // beneath show through; the rest fade as it covers them.
                let floating = self.workspace.read(cx).settings.appearance.floating;
                let fill = bezel::ui::window::content_fill(floating, Theme::of(cx));
                let backing = div()
                    .absolute()
                    .inset_0()
                    .bg(fill)
                    .when(floating, |backing| {
                        backing.rounded(px(Theme::panel_radius()))
                    });
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .child(div().size_full().flex().opacity(1. - t).child(tree))
                    .child(
                        div()
                            .absolute()
                            .left(at(from.origin.x, px(0.)))
                            .top(at(from.origin.y, px(0.)))
                            .w(at(from.size.width, whole.width))
                            .h(at(from.size.height, whole.height))
                            .flex()
                            .child(backing)
                            .child(self.pane(&entry, false, window, cx)),
                    )
                    .into_any_element()
            }
            // A zoomed pane stands over the rest, which keep their places
            // underneath — see [`Space::zoom`].
            (None, Some(entry)) => self.pane(&entry, false, window, cx),
            (None, None) => self.node(&space.tree, &mut Vec::new(), None, window, cx),
        };
        Some(
            div()
                .relative()
                .size_full()
                .min_w_0()
                .min_h_0()
                .flex()
                .child(body)
                .child(measured)
                .into_any_element(),
        )
    }

    /// One node: a pane, or a split of them laid out along its axis. `empty`
    /// is a pane drawn elsewhere, whose place is left as room.
    fn node(
        &mut self,
        node: &Node<Member>,
        path: &mut Vec<usize>,
        empty: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            Node::Leaf { .. } => match node.front() {
                Some(front) if Some(&front) != empty => self.pane(&front, true, window, cx),
                _ => div().flex_1().min_w_0().min_h_0().into_any_element(),
            },
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
                // Floating, half the gap between two cards on each side that
                // faces a sibling: the gutter is centred on the seam, and the
                // shares stay fractions of the whole split.
                let half = px(root::card_gap(floating) / 2.);
                let last = children.len() - 1;
                for (ix, child) in children.iter().enumerate() {
                    path.push(ix);
                    let body = self.node(child, path, empty, window, cx);
                    path.pop();
                    let (before, after) = (
                        if ix > 0 { half } else { px(0.) },
                        if ix < last { half } else { px(0.) },
                    );
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
                                Split::Horizontal => el
                                    .w(relative(child.ratio() as f32))
                                    .h_full()
                                    .pl(before)
                                    .pr(after),
                                Split::Vertical => el
                                    .h(relative(child.ratio() as f32))
                                    .w_full()
                                    .pt(before)
                                    .pb(after),
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

    /// One pane, on `entry`. `placed` is whether the tree lays it out where
    /// it stands, the place a zoom glides from.
    fn pane(
        &mut self,
        entry: &Member,
        placed: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = match self.pane_on(entry, cx) {
            Some(pane) => pane
                .cached(StyleRefinement::default().flex().flex_col().size_full())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };
        let body = self
            .dock
            .pane(entry.clone(), px(root::CARD_ROW_HEIGHT), body);
        // No fill: flat, the panes sit on the fill the column under the strip
        // paints once; floating, each pane draws its own card.
        let (places, named) = (self.pane_places.clone(), entry.clone());
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .child(body)
            // Only where the tree lays it out: zoomed or gliding, it is not in its
            // place.
            .when(placed, |pane| {
                pane.child(
                    canvas(
                        move |bounds, _, _| {
                            places.borrow_mut().panes.insert(named, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .into_any_element()
    }

    /// The pane showing `entry`.
    fn pane_on(&self, entry: &Member, cx: &App) -> Option<Entity<Pane>> {
        self.panes
            .iter()
            .find(|pane| pane.read(cx).entry == *entry)
            .cloned()
    }

    /// Make a pane for every pane of what the window lays out — the open
    /// space, or the strip's entry in front — and drop the rest, and hand each
    /// its tab.
    pub(crate) fn sync_panes(&mut self, cx: &mut Context<Self>) {
        let Some(space) = self.arrangement(cx).map(Cow::into_owned) else {
            self.panes.clear();
            return;
        };
        let mut kept = Vec::new();
        for front in space.tree.fronts() {
            let pane = match self
                .panes
                .iter()
                .position(|pane| pane.read(cx).entry == front)
            {
                Some(at) => self.panes.remove(at),
                None => {
                    let root = cx.entity().downgrade();
                    let front = front.clone();
                    cx.new(|_| Pane::new(front, root))
                }
            };
            let tab = self
                .leaves
                .iter()
                .find(|leaf| leaf.read(cx).entry == front)
                .cloned();
            pane.update(cx, |pane, cx| {
                if pane.tab != tab {
                    pane.tab = tab;
                    cx.notify();
                }
            });
            kept.push(pane);
        }
        self.panes = kept;
    }

    /// Whether the panes' title rows along the window's top edge are its top
    /// row: while the sidebar lists the spaces, so the strip has no tabs to
    /// draw, and a pane is in front.
    pub(crate) fn panes_on_top(&self, cx: &App) -> bool {
        self.library.is_none()
            && self
                .workspace
                .read(cx)
                .settings
                .appearance
                .spaces_in_sidebar
            && self.arrangement(cx).is_some()
    }

    /// Tell each pane where it stands against the window's top edge — see
    /// [`Edge`].
    pub(crate) fn sync_edges(&mut self, cx: &mut Context<Self>) {
        let top = match self.panes_on_top(cx) {
            true => self
                .arrangement(cx)
                .map(|space| match space.zoomed() {
                    Some(zoomed) => vec![zoomed],
                    None => space.tree.top_fronts(),
                })
                .unwrap_or_default(),
            false => Vec::new(),
        };
        let on_top = !top.is_empty();
        if let Some(panel) = self.changes.clone()
            && panel.read(cx).on_top != on_top
        {
            panel.update(cx, |panel, cx| {
                panel.on_top = on_top;
                cx.notify();
            });
        }
        let (left, right) = (top.first().cloned(), top.last().cloned());
        let (sidebar_open, panel_open) = (self.sidebar_open, self.changes.is_some());
        for pane in self.panes.clone() {
            let entry = Some(pane.read(cx).entry.clone());
            let edge = Edge {
                top: entry.as_ref().is_some_and(|entry| top.contains(entry)),
                lights: !sidebar_open && entry == left,
                toggle: !panel_open && entry == right,
            };
            if let Some(tab) = pane.read(cx).tab.clone()
                && tab.read(cx).edge != edge
            {
                tab.update(cx, |tab, cx| {
                    tab.edge = edge;
                    cx.notify();
                });
            }
        }
    }

    /// Whether a pane draws the entry `row` names.
    pub(crate) fn on_screen(&self, row: &Row, cx: &App) -> bool {
        let Row::Entry { project, showing } = row else {
            return false;
        };
        // The library stands over every pane.
        if self.library.is_some() {
            return false;
        }
        let workspace = self.workspace.read(cx);
        let (Some(space), Some(at)) = (workspace.arranged(), workspace.project_ix(*project)) else {
            return false;
        };
        space.tree.fronts().iter().any(|shown| {
            workspace
                .showing_of(shown, cx)
                .is_some_and(|(open, shown)| open == at && &shown == showing)
        })
    }

    /// Note a pane as the one most recently entered, which is where a close
    /// falls back to — see [`Cydonia::tab_history`].
    fn remember_front(&mut self, tab: &Member) {
        self.tab_history.retain(|seen| seen != tab);
        self.tab_history.push(tab.clone());
    }

    /// Put the focus on the pane on `entry`.
    pub(crate) fn show_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remember_front(entry);
        self.refocus(entry, window, cx);
        self.reveal_applied_match(cx);
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
            Dragged::Tab(Slot::Space { space }) => self
                .workspace
                .read(cx)
                .space_by_id(space)
                .map(|space| space.label().to_owned())
                .unwrap_or_default(),
        }
    }

    /// A release on a pane: beside it where it lands on an edge, and in front
    /// of its stack anywhere else. Answers the entry the focus is now on.
    pub(crate) fn dock_drop(
        &mut self,
        event: &docking::Drop<Member, Dragged>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Member> {
        let target = &event.pane;
        let arriving = self.dropped(&event.item, cx)?;
        if arriving == *target {
            return None;
        }
        let side = match event.zone {
            docking::Zone::Join => None,
            docking::Zone::Left => Some(Side::Left),
            docking::Zone::Right => Some(Side::Right),
            docking::Zone::Top => Some(Side::Above),
            docking::Zone::Bottom => Some(Side::Below),
        };
        self.workspace.update(cx, |workspace, cx| match side {
            Some(side) => workspace.arrange(target, &arriving, side, cx),
            None => workspace.stack_pane(target, &arriving, cx),
        });
        self.refocus(&arriving, window, cx);
        Some(arriving)
    }

    /// The ways the pane on `entry` can move across a seam: each side with a
    /// pane across it, its label and its arrow. Empty for an entry no space in
    /// front holds beside another.
    pub(crate) fn pane_moves(
        &self,
        entry: &Member,
        cx: &App,
    ) -> Vec<(Side, &'static str, &'static [u8])> {
        if self.zoomed(entry, cx).is_none() {
            return Vec::new();
        }
        [
            (Side::Left, "Move left", icons::arrows::ArrowLeft),
            (Side::Right, "Move right", icons::arrows::ArrowRight),
            (Side::Above, "Move up", icons::arrows::ArrowUp),
            (Side::Below, "Move down", icons::arrows::ArrowDown),
        ]
        .into_iter()
        .filter(|(side, _, _)| {
            self.workspace
                .read(cx)
                .neighbour_pane(entry, *side)
                .is_some()
        })
        .collect()
    }

    /// [`Self::pane_moves`] as rows of a menu.
    pub(crate) fn move_rows(&self, entry: &Member, cx: &App) -> Vec<(Item, menu::Act)> {
        self.pane_moves(entry, cx)
            .into_iter()
            .map(|(side, label, icon)| {
                let on = entry.clone();
                menu::row::<Self>(
                    Item::action(label).with_icon(icon),
                    move |this, window, cx| this.move_pane(&on, side, window, cx),
                )
            })
            .collect()
    }

    /// The stack of the pane on `entry` as rows of a menu: one per entry, the
    /// one in front checked, each bringing its entry to the front; then the
    /// entry in front out to a tab of its own.
    pub(crate) fn stack_rows(&self, entry: &Member, cx: &App) -> Vec<(Item, menu::Act)> {
        let workspace = self.workspace.read(cx);
        let Some(space) = workspace.active_space() else {
            return Vec::new();
        };
        let front = space.front_of(entry);
        let stack = space.stack_of(entry);
        let behind = stack
            .iter()
            .find(|held| Some(*held) != front.as_ref())
            .cloned();
        let mut rows: Vec<_> = stack
            .into_iter()
            .map(|held| {
                let title = self
                    .member_toolbar(&held, cx)
                    .map(|toolbar| toolbar.title)
                    .unwrap_or_default();
                let item = Item::action(title)
                    .with_icon(icon::entry_icon(&held, workspace, cx))
                    .checked(front.as_ref() == Some(&held));
                menu::row::<Self>(item, move |this, window, cx| {
                    this.stack_steps = Some(held.clone());
                    this.bring_forward(&held, window, cx)
                })
            })
            .collect();
        // Out of the stack: to a tab of its own on the strip, or, with the
        // spaces in the sidebar and no strip drawn, to a pane of its own
        // beside this one.
        let out = match (front, workspace.settings.appearance.spaces_in_sidebar) {
            (Some(front), false) => Some(menu::row::<Self>(
                Item::action("Move to a tab of its own").with_icon(icons::layout::PanelTop),
                move |this, window, cx| this.join_strip(front.clone(), window, cx),
            )),
            (Some(front), true) => behind.map(|behind| {
                menu::row::<Self>(
                    Item::action("Move to a pane of its own").with_icon(icons::layout::Columns2),
                    move |this, window, cx| this.split_out(&front, &behind, window, cx),
                )
            }),
            (None, _) => None,
        };
        if let Some(out) = out {
            rows.push(menu::row::<Self>(Item::Separator, |_, _, _| {}));
            rows.push(out);
        }
        rows
    }

    /// Take `entry` out of its pane's stack into a pane of its own to the
    /// right of the one `beside` stays in, and focus it.
    fn split_out(
        &mut self,
        entry: &Member,
        beside: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.relocate_pane(entry, beside, Side::Right, cx)
        });
        self.show_pane(entry, window, cx);
    }

    /// Bring the entry `step` along from the focused one in its pane's stack to
    /// the front, wrapping at either end.
    pub(crate) fn step_stack(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.focused_entry(cx) else {
            return;
        };
        let stack = self.workspace.read(cx).stack_of(&entry);
        let Some(at) = stack.iter().position(|held| *held == entry) else {
            return;
        };
        let next = (at as isize + step).rem_euclid(stack.len() as isize) as usize;
        if next != at {
            self.bring_forward(&stack[next].clone(), window, cx);
        }
    }

    /// Whether ⌃⇥ steps the focused pane's stack rather than the strip: an
    /// entry of that stack is [`Cydonia::stack_steps`].
    pub(crate) fn steps_stack(&self, cx: &App) -> bool {
        let (Some(focused), Some(picked)) = (self.focused_entry(cx), self.stack_steps.as_ref())
        else {
            return false;
        };
        let stack = self.workspace.read(cx).stack_of(&focused);
        stack.len() > 1 && stack.contains(picked)
    }

    /// Put `entry` in front of its stack, or of the strip, and the focus on it.
    fn bring_forward(&mut self, entry: &Member, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.set_front(entry, cx));
        self.show_pane(entry, window, cx);
    }

    /// Exchange a pane with the one across the seam, and keep the focus on it
    /// — the pane moved, not the attention.
    pub(crate) fn move_pane(
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

    /// Whether `entry`'s pane is the one zoomed, or nothing where it is no
    /// pane of a space in front beside another.
    pub(crate) fn zoomed(&self, entry: &Member, cx: &App) -> Option<bool> {
        let space = self.workspace.read(cx).active_space()?;
        (space.contains(entry) && space.leaves() > 1)
            .then(|| space.zoomed().is_some_and(|at| at == *entry))
    }

    /// Stand a pane over the others, or put it back, gliding between its
    /// place and the whole of the space.
    pub(crate) fn toggle_zoom(&mut self, entry: &Member, cx: &mut Context<Self>) {
        let expanding = self
            .arrangement(cx)
            .and_then(|space| space.zoomed())
            .as_ref()
            != Some(entry);
        let places = self.pane_places.borrow();
        self.zoom_glide = places
            .space
            .zip(places.panes.get(entry).copied())
            .filter(|_| !cx.reduced_motion())
            .map(|(space, pane)| ZoomGlide {
                entry: entry.clone(),
                from: Bounds {
                    origin: pane.origin - space.origin,
                    size: pane.size,
                },
                expanding,
                since: Instant::now(),
            });
        drop(places);
        self.workspace
            .update(cx, |workspace, cx| workspace.zoom_pane(entry, cx));
        cx.notify();
    }

    /// Drop one pane from the arrangement.
    pub(crate) fn close_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Where the focus lands: where it is, unless it was on the pane
        // closing. Then on the pane entered before it, or else the space's
        // last.
        let entries = self
            .arrangement(cx)
            .map(|space| space.tree.fronts())
            .unwrap_or_default();
        let kept = self
            .focused_entry(cx)
            .filter(|focused| focused != entry)
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
            self.refocus(&entry, window, cx);
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
        let panes = match self.arrangement(cx) {
            Some(space) => space.tree.fronts(),
            None => return,
        };
        if panes.len() < 2 {
            return;
        }
        let here = self.focused_entry(cx);
        let at = here
            .and_then(|here| panes.iter().position(|named| *named == here))
            .unwrap_or(0);
        let landing = (at as isize + step).rem_euclid(panes.len() as isize) as usize;
        let Some(pane) = panes.get(landing).cloned() else {
            return;
        };
        self.refocus(&pane, window, cx);
    }

    /// Close the focused pane. The keyboard's half of the `×` in its title row.
    pub(crate) fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.focused_entry(cx) else {
            return;
        };
        self.close_pane(&entry, window, cx);
    }

    /// Take an entry that is going away — archived or deleted — out of the
    /// open space, and land on the top-left pane.
    pub(crate) fn put_away_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_pane(entry, window, cx);
        let Some(pane) = self
            .arrangement(cx)
            .and_then(|space| space.tree.fronts().first().cloned())
        else {
            return;
        };
        self.refocus(&pane, window, cx);
    }

    /// Stand the pane in front over the others, or put it back.
    pub(crate) fn zoom_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.focused_entry(cx) else {
            return;
        };
        self.toggle_zoom(&entry, cx);
    }

    /// Open a space: the window is arranged by it until an entry no space
    /// holds is opened.
    pub(crate) fn open_space(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.library = None;
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_space(ix, cx);
        });
        // The focus lands on the pane the space last had focused, or else the
        // first it lays out, at its top left.
        let front = self
            .workspace
            .read(cx)
            .active_space()
            .and_then(|space| space.focused_front());
        match front {
            Some(entry) => self.refocus(&entry, window, cx),
            None => self.sync_leaves(window, cx),
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
        self.stack_steps = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.join_strip(&arriving, cx));
        self.show_pane(&arriving, window, cx);
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
/// project and which of its things — two projects can hold the same id — or
/// an app by its id.
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

#[cfg(test)]
#[path = "../../../tests/unit/pane_footer.rs"]
mod pane_footer_tests;
