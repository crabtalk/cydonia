//! The drawer a pane pulls up from its foot, holding one thing at a time: a
//! board's card, or an entry a link names.
//!
//! The drawer is the pane's, held on its [`Tab`]; what it
//! holds is a [`Peek`].

use super::Tab;
use crate::{
    model::{
        project::ProjectId,
        workspace::{Entry as SpaceEntry, Member, Opening, Showing, references::Part},
    },
    view::{
        board::{BoardView, DrawerAt, PaneState},
        kit::watch::Watch,
        sidebar::Row,
        window::{reach::InWindow as _, root::Cydonia},
    },
};
use artifact::{board::Page, reference::Reference};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, EntityId, FocusHandle, KeyBinding, Pixels,
        ScrollHandle, SharedString, Stateful, Window, actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        floating, icons,
        scroll::{self, Axes},
        tooltip::Tooltip,
        widgets::Buttons,
    },
};
use std::{cell::Cell, rc::Rc};

const DRAWER_CONTEXT: &str = "CydoniaDrawer";

/// The least the drawer is dragged down to, short of the pane itself.
const MIN_HEIGHT: f32 = 160.;

actions!(cydonia_drawer, [CloseDrawer]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", CloseDrawer, Some(DRAWER_CONTEXT))]
}

/// What a pane's drawer holds.
#[derive(Clone)]
pub(crate) enum Peek {
    /// The card or the description open in one of the pane's board views —
    /// see [`BoardView::open`].
    Page(Entity<BoardView>),
    Entry(Entry),
}

impl Peek {
    /// What the drawer draws: the board view's open page, or what
    /// `entry_face` draws for an entry, scrolled by `scroll`.
    pub(crate) fn face<V: 'static>(
        self,
        scroll: &ScrollHandle,
        window: &mut Window,
        cx: &mut Context<V>,
        entry_face: impl FnOnce(&Entry, &ScrollHandle, &mut Window, &mut Context<V>) -> Option<Face>,
    ) -> Option<Face> {
        match self {
            Self::Page(view) => view.update(cx, |view, cx| view.page_face(window, cx)),
            Self::Entry(entry) => entry_face(&entry, scroll, window, cx),
        }
    }
}

/// An entry opened from a `cydonia://` link, or from the library.
#[derive(Clone)]
pub struct Entry {
    /// The reference as the link wrote it, `project#12` or `project#12:5-7`.
    pub(crate) reference: String,
    /// The drawer's own view of a session drawn live: its scroll and its
    /// selection are not the session's pane's.
    pub(crate) session: Option<Entity<crate::view::component::transcript::SessionView>>,
}

/// One pane's drawer.
pub struct Drawer {
    pub(crate) peek: Peek,
    pub(crate) scroll: ScrollHandle,
    pub(crate) focus: FocusHandle,
    /// Where the drawer was laid out last frame.
    pub(crate) bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
    pub(crate) size: Size,
    /// How far below the drawer's top edge the resize grip was pressed, while
    /// it is held.
    pub(crate) resize_grab: Option<Pixels>,
    /// Set when the card a drawer holds is to be scrolled clear of it; the
    /// board view taking the reveal clears it.
    pub(crate) reveal: Rc<Cell<bool>>,
    /// The box the drawer is laid out against.
    pane_bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
    /// The board an entry drawer peeks.
    board: Watch,
}

impl Drawer {
    pub(crate) fn new(peek: Peek, focus: FocusHandle) -> Self {
        Self {
            peek,
            scroll: ScrollHandle::new(),
            focus,
            bounds: Default::default(),
            size: Size::default(),
            resize_grab: None,
            reveal: Rc::new(Cell::new(true)),
            pane_bounds: Default::default(),
            board: Watch::default(),
        }
    }

    /// A button in the drawer's head row.
    pub(crate) fn action(
        id: &'static str,
        glyph: &'static [u8],
        label: impl Into<SharedString>,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx);
        theme
            .ghost(id)
            .debug_selector(move || id.to_owned())
            .flex_none()
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(Theme::control_radius()))
            .child(
                icons::icon(glyph)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .tooltip({
                let label = label.into();
                move |window, cx| Tooltip::text(label.clone(), window, cx)
            })
    }

    /// The board view whose open page the drawer holds.
    pub(crate) fn page(&self) -> Option<&Entity<BoardView>> {
        match &self.peek {
            Peek::Page(view) => Some(view),
            Peek::Entry(_) => None,
        }
    }

    fn entry(&self) -> Option<&Entry> {
        match &self.peek {
            Peek::Entry(entry) => Some(entry),
            Peek::Page(_) => None,
        }
    }
}

/// The share of the pane the drawer stands over.
#[derive(Clone, Copy)]
pub(crate) struct Size {
    fraction: f32,
    pub(crate) expanded: bool,
}

impl Default for Size {
    fn default() -> Self {
        Self {
            fraction: 0.5,
            expanded: false,
        }
    }
}

impl Size {
    fn fraction(&self, pane_height: Pixels) -> f32 {
        if self.expanded {
            return 1.;
        }
        let height = f32::from(pane_height);
        if height <= 0. {
            return self.fraction;
        }
        self.fraction.max((MIN_HEIGHT / height).min(1.)).min(1.)
    }

    fn resize(&mut self, pointer_y: Pixels, pane: gpui::Bounds<Pixels>) {
        let height = f32::from(pane.size.height);
        if height <= 0. {
            return;
        }
        self.fraction = (f32::from(pane.bottom() - pointer_y) / height)
            .clamp((MIN_HEIGHT / height).min(1.), 1.);
        self.expanded = false;
    }
}

/// What "open in pane" does.
pub(crate) type OpenIn = Rc<dyn Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>)>;

/// What the drawer draws for the thing it holds, inside its chrome.
pub(crate) struct Face {
    /// The head row's start: what the thing is.
    pub(crate) lead: Vec<AnyElement>,
    /// Buttons of the thing's own, before the drawer's.
    pub(crate) actions: Vec<AnyElement>,
    /// A line under the head row.
    pub(crate) notice: Option<AnyElement>,
    pub(crate) body: AnyElement,
    /// Whether the body scrolls with [`Drawer::scroll`]; a body that does not
    /// fills the drawer and scrolls itself.
    pub(crate) scrolls: bool,
    /// What "open in pane" does, where the thing has a pane of its own.
    pub(crate) open: Option<OpenIn>,
}

impl Tab {
    /// The drawer the pane shows. A card drawer shows while its card is on its
    /// board or still being written.
    pub(crate) fn drawer_shown(&self, cx: &App) -> Option<&Drawer> {
        let drawer = self.drawer.as_ref()?;
        match &drawer.peek {
            Peek::Entry(_) => Some(drawer),
            Peek::Page(view) => view.read(cx).page_live(cx).then_some(drawer),
        }
    }

    /// Put the drawer away and give the focus back to the pane.
    pub(crate) fn close_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_drawer(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Put the drawer away, saving the card it had open.
    pub(crate) fn drop_drawer(&mut self, cx: &mut Context<Self>) {
        if let Some(drawer) = self.drawer.take()
            && let Peek::Page(view) = &drawer.peek
        {
            view.update(cx, |view, cx| view.close(cx));
        }
        cx.notify();
    }

    /// Open `page` of `board` in the drawer, or put the drawer away where it
    /// already shows it.
    pub(crate) fn open_page(
        &mut self,
        board: SpaceEntry,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = self.board_view(board, cx);
        let peeked = self.drawer.as_ref().and_then(Drawer::page) == Some(&view);
        if peeked && view.read(cx).opened() == Some(&page) {
            return self.close_drawer(window, cx);
        }
        match self.drawer.as_mut().filter(|_| peeked) {
            Some(drawer) => {
                drawer.scroll = ScrollHandle::new();
                drawer.reveal.set(true);
            }
            None => {
                self.drop_drawer(cx);
                self.drawer = Some(Drawer::new(Peek::Page(view.clone()), cx.focus_handle()));
            }
        }
        let drawer = self.drawer.as_ref().unwrap();
        let (focus, scroll) = (drawer.focus.clone(), drawer.scroll.clone());
        view.update(cx, |view, cx| view.open(page, scroll, cx));
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Open `entry` in the drawer, at the size the drawer up now stands at.
    pub(crate) fn open_entry(&mut self, entry: Entry, window: &mut Window, cx: &mut Context<Self>) {
        let size = self.drawer.as_ref().map(|drawer| drawer.size);
        self.drop_drawer(cx);
        let mut drawer = Drawer::new(Peek::Entry(entry), cx.focus_handle());
        if let Some(size) = size {
            drawer.size = size;
        }
        let board = drawer.entry().and_then(|entry| {
            let workspace = self.workspace.read(cx);
            match workspace.resolve(&entry.reference, cx).ok()?.showing {
                Showing::Board(id) => workspace.board_entity(&id).cloned(),
                _ => None,
            }
        });
        drawer.board.set(board.as_ref(), cx);
        let focus = drawer.focus.clone();
        self.drawer = Some(drawer);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Whether the drawer shows the entry `reference` names.
    pub(crate) fn peeks(&self, reference: &str) -> bool {
        self.drawer
            .as_ref()
            .and_then(Drawer::entry)
            .is_some_and(|entry| entry.reference == reference)
    }

    /// A press on a board the drawer and the cards did not take: the drawer
    /// goes, and the focus with it back to the pane if it was in the drawer.
    pub(crate) fn board_pressed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(opened) = self.drawer.as_ref() else {
            return;
        };
        // The focus goes back to the board only from the drawer: the click may
        // have put it in a field of its own.
        let inside = opened.focus.contains_focused(window, cx)
            || opened
                .page()
                .is_some_and(|view| view.read(cx).draft_focused(window, cx));
        self.drop_drawer(cx);
        if inside {
            window.focus(&self.focus, cx);
        }
    }

    /// What the pane holds that its board view `view` draws by — see
    /// [`PaneState`]. A drawer peeking `view` itself is handed over whether its
    /// card shows or not: `view` is drawing, and only it can tell.
    pub(crate) fn pane_state(&self, view: EntityId, cx: &App) -> PaneState {
        let drawer = self.drawer.as_ref().and_then(|drawer| {
            let own = drawer.page().is_some_and(|card| card.entity_id() == view);
            let shown = own || self.drawer_shown(cx).is_some();
            shown.then(|| DrawerAt {
                own,
                top: drawer.bounds.get().top(),
                height: drawer.bounds.get().size.height,
                expanded: drawer.size.expanded,
                grabbed: drawer.resize_grab.is_some(),
                reveal: drawer.reveal.clone(),
            })
        });
        PaneState { drawer }
    }

    /// The drawer, over the pane it is drawn in, drawing `face`: everything
    /// that goes in a `relative` box covering the pane's body, after the body.
    pub(crate) fn drawer_layer(&self, face: Face, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(drawer) = self.drawer_shown(cx) else {
            return Vec::new();
        };
        let pane_bounds = drawer.pane_bounds.clone();
        let grab = drawer.resize_grab;
        let mut layer = vec![
            gpui::canvas(
                move |measured, window, _| {
                    if pane_bounds.replace(measured) != measured {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .into_any_element(),
        ];
        layer.extend(self.drawer_chrome(face, cx));
        layer.extend(grab.map(|grab| {
            let release = move |this: &mut Self,
                                _: &gpui::MouseUpEvent,
                                _: &mut Window,
                                cx: &mut Context<Self>| {
                if let Some(drawer) = &mut this.drawer {
                    drawer.resize_grab = None;
                    cx.notify();
                }
                cx.stop_propagation();
            };
            floating::layer("drawer-resizing")
                .inset_0()
                .cursor_row_resize()
                .on_mouse_move(
                    cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                        if let Some(drawer) = &mut this.drawer {
                            if event.pressed_button == Some(gpui::MouseButton::Left) {
                                drawer
                                    .size
                                    .resize(event.position.y - grab, drawer.pane_bounds.get());
                            } else {
                                drawer.resize_grab = None;
                            }
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(gpui::MouseButton::Left, cx.listener(release))
                .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(release))
                .into_any_element()
        }));
        layer
    }

    fn drawer_chrome(&self, face: Face, cx: &mut Context<Self>) -> Option<AnyElement> {
        let drawer = self.drawer.as_ref()?;
        let theme = Theme::of(cx).clone();
        let expanded = drawer.size.expanded;
        let scroll = drawer.scroll.clone();
        let focus = drawer.focus.clone();
        let bounds = drawer.bounds.clone();
        let reveal = drawer.page().map(|_| drawer.reveal.clone());
        let focus_on = self.entry.clone();
        let body = match face.scrolls {
            true => div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(
                    scroll::pane("drawer-body", Axes::Vertical)
                        .size_full()
                        .track_scroll(&scroll)
                        .p(px(16.))
                        .child(face.body),
                )
                .child(bezel::ui::scroll::Overlay::new(
                    "drawer-scroll",
                    &scroll,
                    gpui::Axis::Vertical,
                )),
            false => div()
                .flex_1()
                .min_h_0()
                .relative()
                .flex()
                .flex_col()
                .child(face.body),
        };
        Some(
            scroll::contain_wheel(floating::layer("drawer"), Axes::Both)
                .debug_selector(|| "drawer".into())
                .key_context(DRAWER_CONTEXT)
                .track_focus(&drawer.focus)
                // Before the editor takes its own press, which the pane's
                // focus would otherwise take back off it.
                .capture_any_mouse_down({
                    let root = self.root.clone();
                    move |_, window, cx| {
                        root.update(cx, |root, cx| root.focus_pane(&focus_on, window, cx))
                            .ok();
                    }
                })
                .on_mouse_down(gpui::MouseButton::Left, {
                    move |_, window, cx| {
                        if !focus.contains_focused(window, cx) {
                            window.focus(&focus, cx);
                        }
                        cx.stop_propagation();
                    }
                })
                .on_action(cx.listener(move |this, _: &CloseDrawer, window, cx| {
                    this.close_drawer(window, cx);
                }))
                .bottom_0()
                .left_0()
                .right_0()
                .h(gpui::relative(
                    drawer.size.fraction(drawer.pane_bounds.get().size.height),
                ))
                .bg(theme.surface_raised)
                .child(bezel::ui::cover::cover())
                .rounded_t(px(if expanded {
                    0.
                } else {
                    Theme::control_radius()
                }))
                .border_t_1()
                .border_l_1()
                .border_r_1()
                .border_color(theme.border)
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.12),
                    offset: gpui::point(px(0.), px(-4.)),
                    blur_radius: px(16.),
                    spread_radius: px(-4.),
                    inset: false,
                }])
                .text_color(theme.text)
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .id("drawer-resize")
                        .debug_selector(|| "drawer-resize".into())
                        .group("drawer-resize")
                        .flex_none()
                        .h(px(9.))
                        .w_full()
                        .cursor_row_resize()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w(px(32.))
                                .h(px(2.))
                                .rounded_full()
                                .bg(theme.text_faint.opacity(0.4))
                                .group_hover("drawer-resize", |el| el.bg(theme.text_muted)),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                if let Some(drawer) = &mut this.drawer {
                                    drawer.resize_grab =
                                        Some(event.position.y - drawer.bounds.get().top());
                                    cx.notify();
                                }
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(16.))
                        .pb(px(6.))
                        .flex_wrap()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .children(face.lead)
                        .child(div().flex_1())
                        .children(face.actions)
                        .child(
                            Drawer::action(
                                "drawer-expand",
                                if expanded {
                                    icons::arrows::Shrink
                                } else {
                                    icons::arrows::Expand
                                },
                                if expanded {
                                    "Restore drawer"
                                } else {
                                    "Expand drawer"
                                },
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(drawer) = &mut this.drawer {
                                        drawer.size.expanded = !drawer.size.expanded;
                                        let expanded = drawer.size.expanded;
                                        if drawer.page().is_some() {
                                            drawer.reveal.set(!expanded);
                                        }
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .children(face.open.map(|open| {
                            Drawer::action(
                                "drawer-open",
                                icons::layout::Maximize,
                                "Open in pane",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.drop_drawer(cx);
                                    let open = open.clone();
                                    this.defer_window(window, cx, move |root, window, cx| {
                                        open(root, window, cx)
                                    });
                                },
                            ))
                        }))
                        .child(
                            Drawer::action("drawer-close", icons::notifications::X, "Close", cx)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_drawer(window, cx);
                                })),
                        ),
                )
                .children(face.notice)
                .child(body)
                .child(
                    gpui::canvas(
                        move |measured, window, _| {
                            let old = bounds.replace(measured);
                            if old != measured {
                                if !expanded && let Some(reveal) = &reveal {
                                    reveal.set(true);
                                }
                                window.request_animation_frame();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element(),
        )
    }
}

impl Cydonia {
    /// Put `on`'s drawer away and give the focus back to its pane.
    pub(crate) fn close_drawer(
        &mut self,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.leaf_of(on, cx).cloned() {
            pane.update(cx, |pane, cx| pane.close_drawer(window, cx));
        }
    }

    /// Hold the rows of every table a drawer shows, so its preview has a page
    /// to draw — see [`crate::model::project::Project::hold_pages`].
    pub(crate) fn hold_drawn_tables(&mut self, cx: &mut Context<Self>) {
        let held: Vec<(ProjectId, String)> = {
            let workspace = self.workspace.read(cx);
            self.leaves
                .iter()
                .filter_map(|pane| pane.read(cx).drawer.as_ref()?.entry().cloned())
                .filter_map(|entry| {
                    let resolved = workspace.resolve(&entry.reference, cx).ok()?;
                    match resolved.showing {
                        Showing::Table(key) => Some((resolved.project, key)),
                        _ => None,
                    }
                })
                .collect()
        };
        self.workspace
            .update(cx, |workspace, _| workspace.hold_pages(&held));
    }

    /// Open the entry `reference` names in `on`'s drawer, or put the drawer
    /// away where it already shows it. A card opens as its board opens it.
    pub(crate) fn peek(
        &mut self,
        on: Option<&Member>,
        reference: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(named) = self.named(reference, cx) else {
            return;
        };
        if let Some(Part::Card(card)) = &named.part {
            let board = named
                .row
                .located(self.workspace.read(cx))
                .and_then(|(project, at)| self.workspace.read(cx).board_member(project, at))
                .and_then(Member::into_entry);
            if let Some(board) = board {
                self.open_page(on, board, Page::Card(card.id.clone()), window, cx);
            }
            return;
        }
        if self
            .leaf_of(on, cx)
            .is_some_and(|pane| pane.read(cx).peeks(reference))
        {
            return self.close_drawer(on, window, cx);
        }
        // An article or a board a pane already shows opens in that pane.
        let paned = matches!(
            &named.row,
            Row::Entry {
                showing: Showing::Article(_) | Showing::Board(_),
                ..
            }
        );
        if paned && self.on_screen(&named.row, cx) {
            return self.open_row(&named.row, Opening::Preview, window, cx);
        }
        match &named.row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => {
                let id = *id;
                self.workspace
                    .update(cx, |workspace, cx| workspace.load_history(id, cx));
            }
            Row::Entry {
                showing: Showing::Article(_),
                ..
            } => {
                let Some((project, ix)) = named.row.located(self.workspace.read(cx)) else {
                    return;
                };
                self.workspace
                    .update(cx, |workspace, cx| workspace.load_article(project, ix, cx));
            }
            _ => {}
        }
        let session = match &named.row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => {
                let (id, workspace, root) = (*id, self.workspace.clone(), cx.entity().downgrade());
                Some(cx.new(|cx| {
                    crate::view::component::transcript::SessionView::new(
                        id,
                        None,
                        crate::view::component::transcript::Drawn::Nested,
                        crate::view::kit::link::CARD_WIDTH,
                        workspace,
                        root,
                        cx,
                    )
                }))
            }
            _ => None,
        };
        let entry = Entry {
            reference: reference.to_owned(),
            session,
        };
        if let Some(pane) = self.leaf_of(on, cx).cloned() {
            pane.update(cx, |pane, cx| pane.open_entry(entry, window, cx));
        }
        cx.notify();
    }

    /// `on`'s drawer, over the pane it is drawn in — see [`Tab::drawer_layer`].
    pub(crate) fn drawer_layer(
        &mut self,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(pane) = self.leaf_of(on, cx).cloned() else {
            return Vec::new();
        };
        let Some((peek, scroll)) = pane
            .read(cx)
            .drawer_shown(cx)
            .map(|drawer| (drawer.peek.clone(), drawer.scroll.clone()))
        else {
            return Vec::new();
        };
        let Some(face) = peek.face(&scroll, window, cx, |entry, scroll, window, cx| {
            Some(self.entry_face(entry, on, scroll, window, cx))
        }) else {
            return Vec::new();
        };
        pane.update(cx, |pane, cx| pane.drawer_layer(face, cx))
    }

    /// What the drawer draws for an entry a link names: a session live or a
    /// run of its turns, an article's document or part of it, or a board's
    /// cards. An article or a board a pane shows is its row.
    pub(crate) fn entry_face(
        &mut self,
        entry: &Entry,
        on: Option<&Member>,
        scroll: &ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Face {
        let theme = Theme::of(cx).clone();
        let named = match self.named(&entry.reference, cx) {
            Ok(named) => named,
            Err(why) => {
                return Face {
                    lead: Vec::new(),
                    actions: Vec::new(),
                    notice: None,
                    body: div()
                        .p(px(16.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_faint)
                        .child(why)
                        .into_any_element(),
                    scrolls: false,
                    open: None,
                };
            }
        };
        let session = match &named.row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => Some(*id),
            _ => None,
        };
        let title = named.row.label(self.workspace.read(cx), cx);
        let lead = vec![
            icons::icon(crate::view::kit::icon::kind_icon(named.kind))
                .size(px(14.))
                .text_color(theme.text_muted)
                .into_any_element(),
            div()
                .min_w_0()
                .text_ellipsis()
                .text_style(TextStyle::Subheadline)
                .text_color(theme.text)
                .child(title)
                .into_any_element(),
            div()
                .font_family(theme.font_mono.clone())
                .child(Reference::entry(None, named.number, None).to_string())
                .into_any_element(),
        ];
        let (body, open): (AnyElement, OpenIn) = match (session, named.part.clone()) {
            (Some(id), None | Some(Part::Passage(_) | Part::Card(_))) => (
                match entry.session.clone() {
                    Some(view) => self.session_transcript(
                        view,
                        self.workspace.read(cx).settings.drawer_composer,
                        window,
                        cx,
                    ),
                    None => div().into_any_element(),
                },
                Rc::new(
                    move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.select_session(id, Opening::Preview, window, cx)
                    },
                ),
            ),
            (Some(id), Some(Part::Turns(turns))) => {
                let from = turns.from.saturating_sub(1) as usize;
                (
                    self.excerpt_body(id, turns, None, window, cx),
                    Rc::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            this.select_session(id, Opening::Preview, window, cx);
                            this.reveal_session_turn(id, from, cx);
                        },
                    ),
                )
            }
            (None, Some(Part::Passage(passage))) => {
                let row = named.row.clone();
                let block = passage.blocks.start;
                (
                    self.passage_body(&row, &passage, None, window, cx),
                    Rc::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            this.open_passage(&row, block, window, cx)
                        },
                    ),
                )
            }
            (None, _) => {
                let row = named.row.clone();
                let peek = match &row {
                    Row::Entry {
                        showing: Showing::Article(_) | Showing::Board(_) | Showing::Table(_),
                        ..
                    } if !self.on_screen(&row, cx) => row.located(self.workspace.read(cx)),
                    _ => None,
                };
                let body = peek
                    .and_then(|(project, at)| match &row {
                        Row::Entry {
                            showing: Showing::Board(_),
                            ..
                        } => self.board_peek(project, at, on, scroll, cx),
                        Row::Entry {
                            showing: Showing::Table(_),
                            ..
                        } => self.table_peek(project, at, cx),
                        _ => self.article_peek(project, at, window, cx),
                    })
                    .unwrap_or_else(|| self.entry_row(named, cx));
                (
                    body,
                    Rc::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            this.open_row(&row, Opening::Preview, window, cx)
                        },
                    ),
                )
            }
        };
        Face {
            lead,
            actions: Vec::new(),
            notice: None,
            body,
            scrolls: false,
            open: Some(open),
        }
    }
}
