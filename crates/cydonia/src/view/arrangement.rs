//! Drawing a layout: the panes it arranges, the seams between them, and the
//! edges a drag can drop on.
//!
//! The tree is [`artifact::layout::Node`] — see there for what the shape
//! means. This is only how it lands on the window.

use crate::{
    model::workspace::Showing,
    view::{
        component::{
            divider,
            menu::{self, Menu},
        },
        root::Cydonia,
        sidebar::EntryDrag,
    },
};
use artifact::layout::{Axis as Split, Layout, Member, Node, Side};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, MouseButton, SharedString, Window,
        div, prelude::*, px, relative,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        menu::Item,
        popover,
        tooltip::Tooltip,
        widgets::{Buttons, Content},
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

/// The least of a split a pane may be squeezed to. A pane thinner than this
/// has nothing left to grab it by.
const MIN_SHARE: f64 = 0.08;

/// What an arrival takes of the pane it is dropped on: half, which is also
/// what the split leaves the two of them at.
const HALF: f32 = 0.5;

/// What the pane's name is padded by, and what a bar that is not the window's
/// leading one starts its name at: the fill that marks the focused pane needs
/// room around the text, and room off the pane's own edge so it does not run
/// into the seam.
const TAB_INSET: f32 = 8.;

impl Cydonia {
    /// The layout the window is arranged by, taken whole: the tree is walked
    /// while the workspace is drawn from, so it is cloned out first.
    pub(crate) fn arrangement(&self, cx: &App) -> Option<Layout> {
        self.workspace.read(cx).active_layout().cloned()
    }

    /// The panes of the open layout, or nothing where none is open and the
    /// window is showing one entry.
    pub(crate) fn panes(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let layout = self.arrangement(cx)?;
        // A zoomed pane stands over the rest, which keep their places
        // underneath — see [`Layout::zoom`].
        if let Some(entry) = layout.zoomed() {
            return Some(self.pane(&entry, window, cx));
        }
        Some(self.node(&layout.tree, &mut Vec::new(), window, cx))
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

    /// One pane, on the entry the layout named.
    fn pane(&self, entry: &Member, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The pane at the window's top left, which is the one that has to keep
        // clear of the traffic lights.
        let first = self
            .arrangement(cx)
            .and_then(|layout| layout.entries().first().cloned())
            .as_ref()
            == Some(entry);
        let theme = Theme::of(cx).clone();
        let showing = self.workspace.read(cx).showing_of(entry);
        let key = key_of(entry);
        let held = entry.clone();
        let body = match showing {
            // The entry has gone since the layout named it. The pane says so
            // rather than standing empty: a blank pane reads as a bug, and the
            // layout is about to drop the member anyway — see
            // [`crate::model::workspace::Workspace::prune_layouts`].
            None => theme
                .empty_state(
                    icons::files::File,
                    "This entry has gone",
                    "It was deleted after the layout was made.",
                )
                .into_any_element(),
            Some((project, showing)) => self.pane_body(project, showing, Some(entry), window, cx),
        };
        let composer = match showing {
            Some((_, Showing::Session(_))) => self
                .leaves
                .iter()
                .find(|leaf| leaf.entry.as_ref() == Some(entry))
                .map(|leaf| crate::view::detail::footer(leaf.composer.clone(), None)),
            _ => None,
        };
        let landing = self
            .pane_landing
            .as_ref()
            .filter(|(on, _)| on == entry)
            .map(|(_, side)| *side);
        div()
            .id(SharedString::from(format!("pane-{key}")))
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
            // Which edge the pointer is over decides what a release does, so
            // it is tracked while the drag is in the air and drawn by the mark
            // below.
            .on_drag_move(cx.listener({
                let on = held.clone();
                move |this, event: &DragMoveEvent<EntryDrag>, _, cx| {
                    this.aim_pane(&on, event.bounds, event.event.position, cx);
                }
            }))
            .on_drop(cx.listener({
                let on = held.clone();
                move |this, drag: &EntryDrag, window, cx| {
                    this.drop_entry(&drag.0, &on, window, cx);
                }
            }))
            // Pressing anywhere in a pane is how the focus moves to it, the
            // same way a click into the sidebar selects a row.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let on = held.clone();
                    move |this, _, window, cx| this.focus_pane(&on, window, cx)
                }),
            )
            .child(self.pane_bar(entry, showing, first, &theme, window, cx))
            .child(body)
            .children(composer)
            .children(landing.map(|side| landing_mark(side, &theme)))
            .into_any_element()
    }

    /// The single pane, wrapped so an entry dropped on its edge makes the
    /// layout that puts the two side by side.
    pub(crate) fn lone_pane(&self, body: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(on) = self
            .workspace
            .read(cx)
            .active
            .zip(self.showing(cx))
            .and_then(|(project, pane)| self.member_showing(project, pane, cx))
        else {
            return body;
        };
        let landing = self
            .pane_landing
            .as_ref()
            .filter(|(at, _)| *at == on)
            .map(|(_, side)| *side);
        div()
            .id("lone-pane")
            .size_full()
            .min_w_0()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .on_drag_move(cx.listener({
                let at = on.clone();
                move |this, event: &DragMoveEvent<EntryDrag>, _, cx| {
                    this.aim_pane(&at, event.bounds, event.event.position, cx);
                }
            }))
            .on_drop(cx.listener({
                let at = on.clone();
                move |this, drag: &EntryDrag, window, cx| {
                    this.drop_entry(&drag.0, &at, window, cx);
                }
            }))
            .child(body)
            .children(landing.map(|side| landing_mark(side, &theme)))
            .into_any_element()
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
    /// no layout is open, or where the entry is not one a pane is on.
    pub(crate) fn width_share(&self, entry: Option<&Member>, cx: &App) -> f32 {
        let Some(entry) = entry else {
            return 1.;
        };
        self.arrangement(cx)
            .and_then(|layout| match layout.zoomed() {
                // A zoomed pane has the window to itself.
                Some(zoomed) if zoomed == *entry => Some(1.),
                Some(_) => None,
                None => layout.tree.share_of(entry, Split::Horizontal),
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
    /// brings the sidebar back. Both are the band's when no layout is open —
    /// see [`Self::pane_header`], which this follows.
    #[allow(clippy::too_many_arguments)]
    fn pane_bar(
        &self,
        entry: &Member,
        showing: Option<(usize, Showing)>,
        first: bool,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toolbar = showing.and_then(|(project, showing)| self.toolbar_of(project, showing, cx));
        let focused = self.leaf().entry.as_ref() == Some(entry);
        let key = key_of(entry);
        let held = entry.clone();
        // The lights are the window's and are drawn over whatever is at its
        // top left, so their clearance is taken by the pane that lands there
        // and nowhere another pane can see it. Fullscreen has none.
        let fold = first && !self.sidebar_open;
        let lead = match (first, self.sidebar_open || window.is_fullscreen()) {
            (true, true) => crate::view::root::HEADER_INSET,
            (true, false) => crate::view::root::TOOLBAR_INSET,
            (false, _) => TAB_INSET,
        };
        let title = toolbar
            .as_ref()
            .map(|toolbar| toolbar.title.clone())
            .unwrap_or_default();
        div()
            .flex_none()
            .h(px(crate::view::root::HEADER_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .pl(px(lead))
            .pr(px(6.))
            // The fold belongs to whichever column runs along the window's left
            // edge, so with the sidebar gone it is this pane's.
            .children(fold.then(|| self.fold_toggle(theme.text, cx).into_any_element()))
            .child(
                div()
                    .id(SharedString::from(format!("pane-tab-{key}")))
                    // Sized to the name it carries, not to the bar: a tab
                    // stretched the width of the pane reads as a field to type
                    // into rather than a label.
                    .flex_none()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .h(px(24.))
                    .px(px(TAB_INSET))
                    .rounded(px(Theme::control_radius()))
                    .cursor_pointer()
                    .text_style(TextStyle::Callout)
                    .text_color(match focused {
                        true => theme.text,
                        false => theme.text_muted,
                    })
                    // Where the focus is said, for the whole pane: the panes
                    // are one plane divided and take no fill of their own, so
                    // this name is the only thing carrying it. The sidebar's
                    // pair — see [`crate::view::sidebar::row`].
                    .when(focused, |el| el.bg(theme.element_active))
                    .when(!focused, |el| el.hover(|el| el.bg(theme.element_hover)))
                    .child(div().flex_none().truncate().child(title))
                    .children(
                        toolbar
                            .as_ref()
                            .and_then(|toolbar| toolbar.number)
                            .map(|number| {
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::Caption)
                                    .text_color(theme.text_faint)
                                    .child(format!("#{number}"))
                            }),
                    )
                    .on_click(cx.listener({
                        let on = held.clone();
                        move |this, _, window, cx| this.focus_pane(&on, window, cx)
                    })),
            )
            .child(div().flex_1().min_w_0())
            .child(
                self.menu_button(
                    SharedString::from(format!("pane-menu-{key}")),
                    Some("pane"),
                    icons::icon(icons::layout::Ellipsis)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Pane(key.clone()),
                    cx,
                )
                .children(self.pane_menu(entry, cx)),
            )
            .child(
                theme
                    .ghost(SharedString::from(format!("close-pane-{key}")))
                    .flex_none()
                    .p(px(3.))
                    .invisible()
                    .group_hover("pane", |el| el.visible())
                    .child(
                        icons::icon(icons::math::Minus)
                            .size(px(13.))
                            .text_color(theme.text_muted),
                    )
                    .tooltip(move |window, cx| Tooltip::text("Close pane", window, cx))
                    .on_click(cx.listener({
                        let on = held.clone();
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close_pane(&on, window, cx);
                        }
                    })),
            )
            .into_any_element()
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
    fn pane_menu(&self, entry: &Member, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = key_of(entry);
        if self.menu.as_ref() != Some(&Menu::Pane(key.clone())) {
            return None;
        }
        let zoomed = self
            .arrangement(cx)
            .and_then(|layout| layout.zoomed())
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
            self.menu_card(id, rows, cx),
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
        self.workspace.update(cx, |workspace, cx| {
            workspace.close_pane(entry, cx);
        });
        self.sync_leaves(window, cx);
        if let Some(entry) = self.leaf().entry.clone() {
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
                .unwrap_or_else(|| self.launch(cx)),
            Showing::Table(at) => self
                .table(project, at, on, cx)
                .unwrap_or_else(|| self.launch(cx)),
        }
    }

    /// Which edge of a pane a pointer at this fraction of it is nearest.
    ///
    /// The four answers tile the pane, so there is nowhere in it a release
    /// means nothing — a target that lights up and then does nothing when let
    /// go of is worse than no target at all.
    pub(crate) fn side_at(across: f32, down: f32) -> Side {
        [
            (across, Side::Left),
            (1. - across, Side::Right),
            (down, Side::Above),
            (1. - down, Side::Below),
        ]
        .into_iter()
        .fold((f32::MAX, Side::Left), |(near, held), (at, side)| {
            match at < near {
                true => (at, side),
                false => (near, held),
            }
        })
        .1
    }

    /// Step the focus to the pane next along the arrangement.
    ///
    /// The order is the order the panes are laid out — left to right, and each
    /// column top to bottom — so this walks the window rather than jumping
    /// about it. `select-pane -LRUD` measured off the bounds would be truer to
    /// tmux; it needs each pane's frame, which nothing here keeps yet.
    pub(crate) fn step_pane(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.leaves.len() < 2 {
            return;
        }
        let at = (self.focused as isize + step).rem_euclid(self.leaves.len() as isize) as usize;
        let Some(entry) = self.leaves.get(at).and_then(|leaf| leaf.entry.clone()) else {
            return;
        };
        self.focus_pane(&entry, window, cx);
    }

    /// Close the pane in front, and with it the member the layout named.
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

    /// Open a layout: the window is arranged by it until another entry is
    /// opened on its own.
    pub(crate) fn open_layout(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.open_layout(ix, cx);
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

    /// Note which edge of a pane the pointer is over, so the mark can say
    /// where a release would put what is in the air.
    fn aim_pane(
        &mut self,
        entry: &Member,
        bounds: bezel::gpui::Bounds<bezel::gpui::Pixels>,
        at: bezel::gpui::Point<bezel::gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !bounds.contains(&at) {
            // Left this pane: whichever one the pointer is now inside says so
            // for itself, and a release outside them all means nothing.
            if self
                .pane_landing
                .as_ref()
                .is_some_and(|(on, _)| on == entry)
            {
                self.pane_landing = None;
                cx.notify();
            }
            return;
        }
        let across = f32::from(at.x - bounds.left()) / f32::from(bounds.size.width);
        let down = f32::from(at.y - bounds.top()) / f32::from(bounds.size.height);
        let side = Self::side_at(across, down);
        if self.pane_landing.as_ref() != Some(&(entry.clone(), side)) {
            self.pane_landing = Some((entry.clone(), side));
            cx.notify();
        }
    }

    /// A release on a pane: beside it where the pointer was over an edge, and
    /// on it where it was over the middle.
    fn drop_entry(
        &mut self,
        arriving: &Member,
        target: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, side)) = self.pane_landing.take() else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.arrange(target, arriving, side, cx);
        });
        self.sync_leaves(window, cx);
        self.focus_pane(arriving, window, cx);
        cx.notify();
    }

    /// Put the seam after `at` where the pointer left it.
    fn move_seam(&mut self, path: &[usize], at: usize, fraction: f64, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            let Some(layout) = workspace.active_layout_mut() else {
                return;
            };
            let Some(split) = layout.tree.at_path_mut(path) else {
                return;
            };
            if split.resize(at, fraction, MIN_SHARE) {
                cx.notify();
            }
        });
    }
}

/// The half of the pane a release would give the arrival.
fn landing_mark(side: Side, theme: &Theme) -> AnyElement {
    let mark = div().absolute().bg(theme.text_muted.opacity(0.28));
    match side {
        Side::Left => mark.left_0().top_0().bottom_0().w(relative(HALF)),
        Side::Right => mark.right_0().top_0().bottom_0().w(relative(HALF)),
        Side::Above => mark.top_0().left_0().right_0().h(relative(HALF)),
        Side::Below => mark.bottom_0().left_0().right_0().h(relative(HALF)),
    }
    .into_any_element()
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
