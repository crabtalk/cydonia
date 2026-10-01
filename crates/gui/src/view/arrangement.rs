//! Drawing a space: the panes it arranges, the seams between them, and the
//! edges a drag can drop on.
//!
//! The tree is [`artifact::space::Node`] — see there for what the shape
//! means. This is only how it lands on the window.

use crate::{
    model::workspace::Showing,
    view::{
        chrome,
        component::{
            divider,
            menu::{self, Menu},
        },
        leaf::Pane,
        root::Cydonia,
        sidebar::Dragged,
    },
};
use artifact::space::{Axis as Split, Member, Node, Side, Space};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, MouseButton, SharedString, Window,
        div, prelude::*, px, relative,
    },
    motion::Painter,
    theme::Theme,
    ui::{
        docking, icons, menu::Item, popover, tabs, titlebar::CaptionSide, tooltip::Tooltip,
        widgets::Content,
    },
};

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

/// What a pane's `+` makes.
#[derive(Clone, Copy)]
enum New {
    /// On the agent at this index of the settings.
    Session(usize),
    Board,
    Article,
    Table,
}

/// The least of a split a pane may be squeezed to. A pane thinner than this
/// has nothing left to grab it by.
const MIN_SHARE: f64 = 0.08;

/// What the pane's name is padded by, and what a bar that is not the window's
/// leading one starts its name at: the fill that marks the focused pane needs
/// room around the text, and room off the pane's own edge so it does not run
/// into the seam.
const TAB_INSET: f32 = 8.;

impl Cydonia {
    /// The space the window is arranged by, taken whole: the tree is walked
    /// while the workspace is drawn from, so it is cloned out first.
    pub(crate) fn arrangement(&self, cx: &App) -> Option<Space> {
        self.workspace.read(cx).active_space().cloned()
    }

    /// The panes of the open space, or nothing where none is open and the
    /// window is showing one entry.
    pub(crate) fn panes(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
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
        &self,
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
                        divider::divider(&theme, across)
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
    /// the pane is showing is [`Self::front_of`], and the two are the same
    /// thing only for a pane holding one entry.
    fn pane(&self, entry: &Member, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The pane at the window's top left, which is the one that has to keep
        // clear of the traffic lights.
        let first = self
            .arrangement(cx)
            .and_then(|space| space.entries().first().cloned())
            .as_ref()
            == Some(entry);
        // And the one at its top right, which carries the caption buttons
        // unless the right panel stands between it and the edge.
        let last = self.changes.is_none()
            && self
                .arrangement(cx)
                .and_then(|space| space.zoomed().or_else(|| top_right(&space.tree)))
                .as_ref()
                == Some(entry);
        let theme = Theme::of(cx).clone();
        let stack = self.workspace.read(cx).stack_of(entry);
        let front = self.front_of(entry, &stack);
        let showing = self.workspace.read(cx).showing_of(&front);
        let key = key_of(entry);
        let body = match showing {
            // The entry has gone since the space named it. The pane says so
            // rather than standing empty: a blank pane reads as a bug, and the
            // space is about to drop the member anyway — see
            // [`crate::model::workspace::Workspace::prune_spaces`].
            None => theme
                .empty_state(
                    icons::files::File,
                    "This entry has gone",
                    "It was deleted after the space was made.",
                )
                .into_any_element(),
            Some((project, showing)) => self.pane_body(project, showing, Some(&front), window, cx),
        };
        let composer = match showing {
            Some((_, Showing::Session(id))) => self
                .leaves
                .iter()
                .find(|leaf| leaf.entry.as_ref() == Some(&front))
                .map(|leaf| {
                    crate::view::detail::footer(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            // The band occludes the pane, so the pane's own
                            // press never sees one landing here.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener({
                                    let on = front.clone();
                                    move |this, _, window, cx| this.focus_pane(&on, window, cx)
                                }),
                            )
                            .children(self.plan(Some(id), cx))
                            .children(self.permission(Some(id), cx))
                            .child(leaf.composer.clone()),
                        self.workspace
                            .read(cx)
                            .session(id)
                            .map(|chat| chat.transcript.footer_height.clone()),
                    )
                }),
            _ => None,
        };
        let pane = div()
            .id(SharedString::from(format!("pane-{key}")))
            // With the context but without this, a pane claims chords that
            // never reach it: an action runs through the focused element's
            // ancestors, and a pane showing a board holds nothing that takes
            // the focus — see [`crate::view::leaf::Leaf::focus`].
            // The front's leaf, not the pane's own name: the body, the
            // composer and [`Cydonia::focus_pane`] all answer for the tab in
            // front, and a pane tracking a handle nothing focuses reads as
            // unfocused while the window's focus sits on an element no frame
            // draws — which the root then takes back. See
            // [`Cydonia::leaf_of`], whose fallback is the focused leaf: two
            // panes that both miss would track one handle and both light up.
            .track_focus(&self.leaf_of(Some(&front)).focus.clone())
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
            // No fill: the panes sit on the detail column's one surface, and a
            // wash per pane would draw the arrangement as a row of cards
            // rather than one plane divided. The focus is said on the pane's
            // name — see [`Self::pane_bar`].
            //
            // Pressing anywhere in a pane is how the focus moves to it, the
            // same way a click into the sidebar selects a row.
            //
            // The tab in front, not the pane's name: a leaf is held by what it
            // shows — see [`Cydonia::leaf_of`] — so a pane showing its second
            // tab would be named by a member no leaf answers to, and the press
            // would leave the focus where it was.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let on = front.clone();
                    move |this, _, window, cx| this.focus_pane(&on, window, cx)
                }),
            )
            .child(self.pane_bar(entry, &stack, &front, first, last, &theme, window, cx))
            .child(body)
            .children(composer);
        // By the tab in front: that is what a drop joins or divides.
        self.dock
            .pane(front, px(crate::view::root::HEADER_HEIGHT), pane)
            .into_any_element()
    }

    /// Which of a pane's entries it is showing.
    ///
    /// Kept on the window and not in the file — see [`Cydonia::fronts`] — so a
    /// space reopens with each pane on the first of its strip. What is
    /// remembered falls back to that as well once it is no longer in the
    /// strip, which is what a closed tab leaves behind.
    pub(crate) fn front_of(&self, pane: &Member, stack: &[Member]) -> Member {
        self.fronts
            .get(&key_of(pane))
            .filter(|front| stack.contains(front))
            .or_else(|| stack.first())
            .unwrap_or(pane)
            .clone()
    }

    /// Move `moving` to `to`'s place in their pane's strip. `false`, and
    /// nothing moves, where the two are not in one strip.
    ///
    /// [`Cydonia::fronts`] is keyed by the strip's first entry, so a move that
    /// changes the first carries the pane's front over to the new key.
    fn reorder_tab(&mut self, moving: &Member, to: &Member, cx: &mut Context<Self>) -> bool {
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
        if was != now {
            // A pane is named by its first tab: what is kept under that name
            // follows the pane to its new one.
            if let Some(front) = self.fronts.remove(&key_of(&was)) {
                self.fronts.insert(key_of(&now), front);
            }
            let mut strips = self.strips.borrow_mut();
            if let Some(strip) = strips.remove(&key_of(&was)) {
                strips.insert(key_of(&now), strip);
            }
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
        self.fronts.insert(key_of(pane), tab.clone());
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
    fn lone_member(&self, cx: &App) -> Option<Member> {
        self.workspace
            .read(cx)
            .active
            .zip(self.showing(cx))
            .and_then(|(project, pane)| self.member_showing(project, pane, cx))
    }

    /// A pane's strip state, kept across frames by the pane's name.
    fn strip_of(&self, key: &SharedString, cx: &Context<Self>) -> tabs::Reorder<Dragged> {
        self.strips
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| tabs::Reorder::new(Painter::of(cx)))
            .clone()
    }

    /// The single entry's column, header included, as a pane a drop lands on:
    /// over the header it makes the two tabs, and on an edge the space that
    /// puts them side by side.
    pub(crate) fn lone_pane(&self, column: AnyElement, cx: &App) -> AnyElement {
        match self.lone_member(cx) {
            Some(on) => self
                .dock
                .pane(on, px(crate::view::root::HEADER_HEIGHT), column)
                .into_any_element(),
            None => column,
        }
    }

    /// The member a carried item names, once it has landed.
    fn dropped(&mut self, item: &Dragged, cx: &mut Context<Self>) -> Option<Member> {
        match item {
            Dragged::Tab(member) => Some(member.clone()),
            Dragged::Row(row) => self.landed_row(*row, cx),
        }
    }

    /// What a carried item is called, for the ghost that follows the pointer.
    pub(crate) fn label_of_dragged(&self, item: &Dragged, cx: &App) -> String {
        match item {
            Dragged::Row(row) => self.label_of_row(*row, cx),
            Dragged::Tab(member) => self
                .workspace
                .read(cx)
                .showing_of(member)
                .and_then(|(project, showing)| self.toolbar_of(project, showing, cx))
                .map(|toolbar| toolbar.title)
                .unwrap_or_default(),
        }
    }

    /// A release on a pane: into its strip where it joins, beside it where
    /// it lands on an edge. Answers the pane the arrival is now in front of.
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
        let arriving = self.dropped(&event.item, cx)?;
        if arriving == *target {
            return None;
        }
        let side = match event.zone {
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
    fn member_showing(
        &self,
        project: usize,
        pane: crate::view::leaf::Pane,
        cx: &App,
    ) -> Option<Member> {
        use crate::view::leaf::Pane;
        let workspace = self.workspace.read(cx);
        let open = workspace.projects.get(project)?;
        let showing = match pane {
            Pane::Chat => Showing::Session(open.active?),
            Pane::Board => Showing::Board(open.board?),
            Pane::Article => Showing::Article(open.article?),
            Pane::Table => Showing::Table(open.table?),
        };
        workspace.member_of(project, showing)
    }

    /// How much of the detail column's width a pane has. The whole of it where
    /// no space is open, or where the entry is not one a pane is on.
    pub(crate) fn width_share(&self, entry: Option<&Member>, cx: &App) -> f32 {
        let Some(entry) = entry else {
            return 1.;
        };
        self.arrangement(cx)
            .and_then(|space| match space.zoomed() {
                // A zoomed pane has the window to itself.
                Some(zoomed) if zoomed == *entry => Some(1.),
                Some(_) => None,
                None => space.tree.share_of(entry, Split::Horizontal),
            })
            .unwrap_or(1.) as f32
    }

    /// One pane's bar: what it is on, and the way out of the arrangement.
    ///
    /// A tab rather than a title, because a pane is where an entry is open and
    /// that is what a tab has always meant here — see
    /// [`crate::view::component::panel`], which holds several.
    ///
    /// The first pane is the one at the window's top left, so it carries what
    /// the window puts there: the traffic lights' clearance, and the fold that
    /// brings the sidebar back. Both are the band's when no space is open —
    /// see [`Self::pane_header`], which this follows.
    #[allow(clippy::too_many_arguments)]
    fn pane_bar(
        &self,
        pane: &Member,
        stack: &[Member],
        front: &Member,
        first: bool,
        last: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = key_of(pane);
        // The lights are the window's and are drawn over whatever is at its
        // top left, so their clearance is taken by the pane that lands there
        // and nowhere another pane can see it. Fullscreen has none.
        let fold = first && !self.sidebar_open;
        let left = fold && chrome::has(CaptionSide::Left, window, cx);
        let right = last && chrome::has(CaptionSide::Right, window, cx);
        let lead = match (first, self.sidebar_open || window.is_fullscreen()) {
            _ if left => 0.,
            (true, true) => crate::view::root::HEADER_INSET,
            (true, false) => crate::view::root::TOOLBAR_INSET,
            (false, _) => TAB_INSET,
        };
        let hovered = key.clone();
        let owner = cx.entity().downgrade();
        crate::view::root::band()
            .id(SharedString::from(format!("pane-bar-{key}")))
            .group("pane-bar")
            .relative()
            .child(
                bezel::gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let exited = owner.clone();
                        let key = hovered.clone();
                        window.on_mouse_event(
                            move |_: &bezel::gpui::MouseExitEvent, phase, _, cx| {
                                if phase != bezel::gpui::DispatchPhase::Capture {
                                    return;
                                }
                                let _ = exited.update(cx, |this, cx| {
                                    if this.pane_hovered.as_ref() == Some(&key) {
                                        this.pane_hovered = None;
                                        cx.notify();
                                    }
                                });
                            },
                        );
                        let owner = owner.clone();
                        let hovered = hovered.clone();
                        // The scrollbar blocks hitbox hover beneath it, but is
                        // still inside the bar. Track the bar's bounds instead.
                        window.on_mouse_event(
                            move |event: &bezel::gpui::MouseMoveEvent, phase, _, cx| {
                                if phase != bezel::gpui::DispatchPhase::Capture {
                                    return;
                                }
                                let _ = owner.update(cx, |this, cx| {
                                    match (
                                        bounds.contains(&event.position),
                                        this.pane_hovered.as_ref() == Some(&hovered),
                                    ) {
                                        (true, false) => this.pane_hovered = Some(hovered.clone()),
                                        (false, true) => this.pane_hovered = None,
                                        _ => return,
                                    }
                                    cx.notify();
                                });
                            },
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
            .w_full()
            .gap(px(2.))
            .pl(px(lead))
            .when(right, |el| el.pr_0())
            .children(
                left.then(|| chrome::caption(CaptionSide::Left, window, cx))
                    .flatten(),
            )
            // The fold belongs to whichever column runs along the window's left
            // edge, so with the sidebar gone it is this pane's.
            .children(fold.then(|| self.fold_toggle(cx).into_any_element()))
            // The tabs in a strip of their own, which scrolls sideways once
            // they no longer fit: the bar's other children are the pane's
            // chrome and keep their places while it does.
            .child({
                let mut strip = tabs::Strip::new();
                for tab in stack {
                    strip.open(Dragged::Tab(tab.clone()));
                }
                strip.activate(&Dragged::Tab(front.clone()));
                self.strip_of(&key, cx)
                    .bar(
                        SharedString::from(format!("pane-strip-{key}")),
                        &strip,
                        stack.iter().map(|tab| {
                            let el = self.pane_tab(pane, tab, tab == front, theme, window, cx);
                            (Dragged::Tab(tab.clone()), el)
                        }),
                    )
                    .on_reorder(cx.listener({
                        let stack = stack.to_vec();
                        move |this, moved: &tabs::Move, _, cx| {
                            if let (Some(moving), Some(to)) =
                                (stack.get(moved.from), stack.get(moved.to))
                            {
                                this.reorder_tab(moving, to, cx);
                            }
                        }
                    }))
            })
            .children(self.pane_project(front, cx).map(|project| {
                self.menu_button(
                    SharedString::from(format!("pane-add-{key}")),
                    Some("pane-bar"),
                    icons::math::Plus,
                    Menu::PaneAdd(key.clone()),
                    cx,
                )
                .children(self.pane_add_menu(pane, project, window, cx))
            }))
            .child(chrome::grip(
                SharedString::from(format!("pane-grip-{key}")),
                &self.drag,
                window,
            ))
            .child(
                self.menu_button(
                    SharedString::from(format!("pane-menu-{key}")),
                    Some("pane"),
                    icons::layout::Ellipsis,
                    Menu::Pane(key.clone()),
                    cx,
                )
                .children(self.pane_menu(pane, window, cx)),
            )
            .children(
                right
                    .then(|| chrome::caption(CaptionSide::Right, window, cx))
                    .flatten(),
            )
            .into_any_element()
    }

    /// One tab: what it is on, and the `×` that takes it out. A right press
    /// opens its entry's menu.
    ///
    /// Every tab carries the close, the pane's first included. There is no
    /// separate control for closing the pane, because there is no separate
    /// thing to close: a pane is its strip, and the last `×` takes the pane
    /// with the tab. The focus mark is on the tab rather than on the pane,
    /// since the panes are one plane divided and take no fill of their own.
    fn pane_tab(
        &self,
        pane: &Member,
        tab: &Member,
        front: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bezel::gpui::Stateful<bezel::gpui::Div> {
        let toolbar = self
            .workspace
            .read(cx)
            .showing_of(tab)
            .and_then(|(project, showing)| self.toolbar_of(project, showing, cx));
        // A number is a project's own, and an arrangement can hold panes from
        // several — so the tab says which project the number is counted in.
        let project = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .find(|open| open.path == tab.project)
            .map(|open| open.name())
            .unwrap_or_default();
        // The pane in front *and* the tab in front of that pane: a background
        // pane's own front tab is not where the window's attention is.
        let focused = front && self.leaf().entry.as_ref() == Some(tab);
        let key = key_of(tab);
        let title = SharedString::from(
            toolbar
                .as_ref()
                .map(|toolbar| toolbar.title.clone())
                .unwrap_or_default(),
        );
        let mut label = tabs::Label::new(title.clone());
        // A number is the badge rather than part of the name: the name
        // truncates and the reference has to survive that.
        if let Some(number) = toolbar.as_ref().and_then(|toolbar| toolbar.number) {
            label = label.with_badge(format!("{project}#{number}"));
        }
        // A tab that is showing but not focused still has to read as the one
        // its pane is on, or a background pane's strip says nothing about what
        // is under it — which is [`tabs::State::Front`].
        let state = match (focused, front) {
            (true, _) => tabs::State::Focused,
            (false, true) => tabs::State::Front,
            (false, false) => tabs::State::Resting,
        };
        tabs::tab(theme, key.clone(), label, state)
            .on_click(cx.listener({
                let (on, shown) = (pane.clone(), tab.clone());
                move |this, _, window, cx| this.show_tab(&on, &shown, window, cx)
            }))
            // The entry's own menu, the one its band's `···` opens, where the
            // press lands.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let menu = Menu::Tab(tab.clone());
                    move |this, press: &bezel::gpui::MouseDownEvent, _, cx| {
                        this.toggle_menu_at(menu.clone(), Some(press.position), cx);
                    }
                }),
            )
            .children(toolbar.and_then(|toolbar| toolbar.entry).and_then(|entry| {
                self.entry_menu(
                    Menu::Tab(tab.clone()),
                    entry.row,
                    entry.archived,
                    window,
                    cx,
                )
            }))
            .child(
                tabs::close(theme, key, tabs::Close::OnHover)
                    .tooltip(move |window, cx| Tooltip::text("Close tab", window, cx))
                    .on_click(cx.listener({
                        let shut = tab.clone();
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close_pane(&shut, window, cx);
                        }
                    })),
            )
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
    fn pane_menu(
        &self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = key_of(entry);
        if self.menu.as_ref() != Some(&Menu::Pane(key.clone())) {
            return None;
        }
        let zoomed = self
            .arrangement(cx)
            .and_then(|space| space.zoomed())
            .is_some_and(|at| at == *entry);

        let mut rows = vec![menu::row(
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
            rows.push(menu::row(
                Item::action(label).with_icon(icon),
                move |this, window, cx| this.move_pane(&on, side, window, cx),
            ));
        }
        let id = SharedString::from(format!("pane-menu-card-{key}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
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
        // The pane the tab was in, if it survives losing it: what the window
        // lands on next is that pane's new front, not whatever leaf happens to
        // sit where the closed one did.
        let stack = self.workspace.read(cx).stack_of(entry);
        let kept = (stack.len() > 1)
            .then(|| {
                // The tab that was in front before this one, so closing walks
                // back the way the reader came. Nothing remembered — a pane
                // never left its first tab — falls back to a neighbour in the
                // strip: the one to its left, or the one to its right for the
                // first.
                let recent = self
                    .tab_history
                    .iter()
                    .rev()
                    .find(|tab| *tab != entry && stack.contains(tab))
                    .cloned();
                recent.or_else(|| {
                    stack
                        .iter()
                        .position(|tab| tab == entry)
                        .map(|at| match at {
                            0 => stack[1].clone(),
                            at => stack[at - 1].clone(),
                        })
                })
            })
            .flatten();
        self.tab_history.retain(|tab| tab != entry);
        // The entry left when this close took the space with it. Without
        // putting the pane on its kind the window drops back to whatever the
        // single pane was last showing, which is not what was on screen.
        let alone = self
            .workspace
            .update(cx, |workspace, cx| workspace.close_pane(entry, cx));
        self.fronts.remove(&key_of(entry));
        if let Some(kept) = &kept
            && let Some(pane) = self.workspace.read(cx).stack_of(kept).first().cloned()
        {
            self.fronts.insert(key_of(&pane), kept.clone());
        }
        // The pane left alone is the one to keep, not the one just closed:
        // [`Cydonia::sync_leaves`] keeps whichever leaf is focused when it
        // finds no space, and the focus is still on the pane going away.
        if let Some((member, _)) = &alone
            && let Some(at) = self
                .leaves
                .iter()
                .position(|leaf| leaf.entry.as_ref() == Some(member))
        {
            self.focused = at;
        }
        self.sync_leaves(window, cx);
        if let Some((_, showing)) = alone {
            self.show_pane(pane_of(showing), cx);
        }
        if let Some(entry) = kept.or_else(|| self.leaf().entry.clone()) {
            self.focused = usize::MAX;
            self.focus_pane(&entry, window, cx);
        }
        cx.notify();
    }

    /// What one pane draws, whichever kind of entry it is on.
    pub(crate) fn pane_body(
        &self,
        project: usize,
        showing: Showing,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match showing {
            Showing::Session(id) => self.conversation(Some(id), on, window, cx),
            Showing::Board(at) => self.board(project, at, on, window, cx),
            // An entry can be named and not yet loaded — an article holds no
            // editor until it is opened. The front door stands in for the
            // moment in between.
            Showing::Article(at) => self
                .article(project, at, on, window, cx)
                .unwrap_or_else(|| self.launch(window, cx)),
            Showing::Table(at) => self
                .table(project, at, on, window, cx)
                .unwrap_or_else(|| self.launch(window, cx)),
        }
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
        let here = self.leaf().entry.clone();
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
        let stack = self.workspace.read(cx).stack_of(pane);
        let front = self.front_of(pane, &stack);
        self.focused = usize::MAX;
        self.focus_pane(&front, window, cx);
    }

    /// Close the tab in front — and with it the pane, where it was the pane's
    /// last. The keyboard's half of the `×` on a tab.
    pub(crate) fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.leaf().entry.clone() else {
            return;
        };
        self.close_pane(&entry, window, cx);
    }

    /// Stand the pane in front over the others, or put it back.
    pub(crate) fn zoom_focused_pane(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.leaf().entry.clone() else {
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
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_space(ix, cx);
        });
        self.sync_leaves(window, cx);
        // The focus lands on the first pane the arrangement lays out, which is
        // the one at its top left.
        if let Some(entry) = self.leaves.first().and_then(|leaf| leaf.entry.clone()) {
            self.focused = 0;
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
            self.fronts.insert(key_of(&pane), arriving.clone());
            self.remember_front(&arriving);
        }
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(&arriving, window, cx);
        cx.notify();
    }

    /// The open project a pane's front tab is in.
    fn pane_project(&self, front: &Member, cx: &App) -> Option<usize> {
        self.workspace
            .read(cx)
            .showing_of(front)
            .map(|(project, _)| project)
    }

    /// What the `+` in a pane's bar starts: the sidebar's `+` for the
    /// project of the pane's front tab, landing as a tab of the pane.
    fn pane_add_menu(
        &self,
        pane: &Member,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = key_of(pane);
        if self.menu.as_ref() != Some(&Menu::PaneAdd(key.clone())) {
            return None;
        }
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        let (sessions, boards, tables) = (features.sessions, features.boards, features.tables);
        let agents: Vec<(String, Option<icons::Icon>)> = workspace
            .settings
            .agents
            .iter()
            .map(|entry| (entry.name.clone(), workspace.agent_icon(&entry.name)))
            .collect();
        let mut rows = Vec::new();
        if sessions && agents.len() > 1 {
            let picks = agents
                .into_iter()
                .enumerate()
                .map(|(at, (name, icon))| {
                    let icon = icon.unwrap_or_else(|| icons::social::MessageCircle.into());
                    let pane = pane.clone();
                    menu::row(
                        Item::action(name).with_icon(icon),
                        move |this, window, cx| {
                            this.new_in_pane(&pane, project, New::Session(at), window, cx)
                        },
                    )
                })
                .collect();
            rows.push(menu::submenu(
                "New session",
                icons::social::MessageCirclePlus,
                picks,
            ));
        } else if sessions && agents.len() == 1 {
            let pane = pane.clone();
            rows.push(menu::row(
                Item::action("New session").with_icon(icons::social::MessageCirclePlus),
                move |this, window, cx| {
                    this.new_in_pane(&pane, project, New::Session(0), window, cx)
                },
            ));
        }
        for (shown, label, icon, kind) in [
            (
                boards,
                "New board",
                icons::Icon::from(icons::development::SquareKanban),
                New::Board,
            ),
            (
                true,
                "New article",
                icons::files::FilePlus.into(),
                New::Article,
            ),
            (tables, "New table", icons::files::Table2.into(), New::Table),
        ] {
            if !shown {
                continue;
            }
            let pane = pane.clone();
            rows.push(menu::row(
                Item::action(label).with_icon(icon),
                move |this, window, cx| this.new_in_pane(&pane, project, kind, window, cx),
            ));
        }
        let id = SharedString::from(format!("pane-add-card-{key}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
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
        let member = self.workspace.update(cx, |workspace, cx| {
            workspace.select_project(project, cx);
            let showing = match kind {
                New::Session(at) => {
                    let agent = workspace.settings.agents.get(at).cloned()?;
                    let id = workspace.new_session(agent, None, cx)?;
                    // A space names its members by file, and a new session
                    // has none until its first turn.
                    workspace.retain_session(id, cx)?;
                    Showing::Session(id)
                }
                New::Article => Showing::Article(workspace.new_article(cx)?),
                New::Table => Showing::Table(workspace.new_table(cx)?),
                New::Board => return None,
            };
            workspace.member_of(project, showing)
        });
        if let Some(member) = member {
            self.add_tab(pane, member, window, cx);
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

/// A member as something that can be an element id: which project, and which
/// of its things. Two projects can hold the same id, so the path is part of
/// it.
fn key_of(member: &Member) -> SharedString {
    SharedString::from(format!(
        "{}::{:?}::{}",
        member.project.display(),
        member.kind,
        member.id
    ))
}

/// A seam's place in the window, as something that can be an element id.
fn seam_id(path: &[usize], at: usize) -> usize {
    path.iter()
        .fold(1usize, |id, step| id.wrapping_mul(31).wrapping_add(*step))
        .wrapping_mul(31)
        .wrapping_add(at)
}

/// The pane one entry is read in.
fn pane_of(showing: Showing) -> Pane {
    match showing {
        Showing::Session(_) => Pane::Chat,
        Showing::Board(_) => Pane::Board,
        Showing::Article(_) => Pane::Article,
        Showing::Table(_) => Pane::Table,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/open_entries.rs"]
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
#[path = "../../tests/unit/pane_footer.rs"]
mod pane_footer_tests;

#[cfg(test)]
#[path = "../../tests/unit/pane_hover.rs"]
mod pane_hover_tests;
