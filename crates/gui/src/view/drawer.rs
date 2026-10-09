//! The drawer a pane pulls up from its foot, holding one thing at a time: a
//! board's card, or an entry a link names.
//!
//! The drawer is the pane's, held on its [`crate::view::leaf::Leaf`]; what it
//! holds is a [`Peek`].

use crate::{
    model::workspace::Showing,
    view::{board::OpenCard, entry_link::load_history, root::Cydonia, sidebar::Row},
};
use artifact::space::Member;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, FocusHandle, KeyBinding, Pixels, ScrollHandle,
        SharedString, Stateful, Window, actions, div, prelude::*, px,
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

actions!(cydonia_drawer, [CloseDrawer]);

const DRAWER_CONTEXT: &str = "CydoniaDrawer";

/// The least the drawer is dragged down to, short of the pane itself.
const MIN_HEIGHT: f32 = 160.;

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", CloseDrawer, Some(DRAWER_CONTEXT))]
}

/// What a pane's drawer holds.
pub enum Peek {
    Card(OpenCard),
    Entry(Entry),
}

/// An entry opened from a `cydonia://` link.
pub struct Entry {
    /// The reference as the link wrote it, `project#12` or `project#12:5-7`.
    pub(crate) reference: String,
    /// The transcript's scroll for a session drawn live. Its own, never the
    /// session's: the session's pane keeps where it was scrolled to.
    pub(crate) list: bezel::ui::list::VariableList<usize>,
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
    /// The box the drawer is laid out against.
    pane_bounds: Rc<Cell<gpui::Bounds<Pixels>>>,
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
            pane_bounds: Default::default(),
        }
    }

    pub(crate) fn card(&self) -> Option<&OpenCard> {
        match &self.peek {
            Peek::Card(card) => Some(card),
            Peek::Entry(_) => None,
        }
    }

    pub(crate) fn card_mut(&mut self) -> Option<&mut OpenCard> {
        match &mut self.peek {
            Peek::Card(card) => Some(card),
            Peek::Entry(_) => None,
        }
    }

    fn entry(&self) -> Option<&Entry> {
        match &self.peek {
            Peek::Entry(entry) => Some(entry),
            Peek::Card(_) => None,
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

impl Cydonia {
    /// The drawer `on` shows. A card drawer shows only over the board it is
    /// on, `board` being the board the pane shows as `(project, index)`.
    pub(crate) fn drawer_shown(
        &self,
        on: Option<&Member>,
        board: Option<(usize, usize)>,
        cx: &App,
    ) -> Option<&Drawer> {
        let drawer = self.leaf_of(on).drawer.as_ref()?;
        match &drawer.peek {
            Peek::Entry(_) => Some(drawer),
            Peek::Card(_) => {
                let (project, at) = board?;
                self.drawer_for(project, at, on, cx)
            }
        }
    }

    /// Put the drawer away and give the focus back to the pane.
    pub(crate) fn close_drawer(
        &mut self,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_drawer(on, cx);
        window.focus(&self.leaf_of(on).focus, cx);
        cx.notify();
    }

    /// Put the drawer away, saving the card it had open.
    pub(crate) fn drop_drawer(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        if let Some(drawer) = self.leaf_of_mut(on).drawer.take()
            && let Peek::Card(card) = &drawer.peek
        {
            self.settle_card_draft(on, &card.board, &card.card, cx);
        }
    }

    /// Open the entry `reference` names in `on`'s drawer, or put the drawer
    /// away where it already shows it.
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
        if self
            .leaf_of(on)
            .drawer
            .as_ref()
            .and_then(Drawer::entry)
            .is_some_and(|entry| entry.reference == reference)
        {
            return self.close_drawer(on, window, cx);
        }
        match &named.row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => {
                let id = *id;
                self.workspace
                    .update(cx, |workspace, _| load_history(workspace, id));
            }
            Row::Entry {
                showing: Showing::Article(_),
                ..
            } => {
                let Some((project, ix)) = self.located(&named.row, cx) else {
                    return;
                };
                // One editor, drawn in one place: an article a pane already
                // shows is the pane's.
                let shown = self
                    .workspace
                    .read(cx)
                    .article_in(project, ix)
                    .is_some_and(|article| self.article_on_screen(&article.path, cx));
                if shown {
                    return self.open_row(&named.row, window, cx);
                }
                self.workspace
                    .update(cx, |workspace, cx| workspace.load_article(project, ix, cx));
            }
            _ => {}
        }
        let size = self.leaf_of(on).drawer.as_ref().map(|drawer| drawer.size);
        self.drop_drawer(on, cx);
        let list = bezel::ui::list::VariableList::default();
        list.state.set_follow_mode(gpui::FollowMode::Tail);
        let mut drawer = Drawer::new(
            Peek::Entry(Entry {
                reference: reference.to_owned(),
                list,
            }),
            cx.focus_handle(),
        );
        if let Some(size) = size {
            drawer.size = size;
        }
        let focus = drawer.focus.clone();
        self.leaf_of_mut(on).drawer = Some(drawer);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// A button in the drawer's head row.
    pub(crate) fn drawer_action(
        &self,
        id: &'static str,
        glyph: &'static [u8],
        label: impl Into<SharedString>,
        cx: &Context<Self>,
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

    /// `on`'s drawer, over the pane it is drawn in: everything that goes in a
    /// `relative` box covering the pane's body, after the body.
    pub(crate) fn drawer_layer(
        &mut self,
        on: Option<&Member>,
        board: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(drawer) = self.drawer_shown(on, board, cx) else {
            return Vec::new();
        };
        let pane_bounds = drawer.pane_bounds.clone();
        let grab = drawer.resize_grab;
        let face = match &drawer.peek {
            Peek::Card(_) => {
                board.and_then(|(project, at)| self.card_face(project, at, on, window, cx))
            }
            Peek::Entry(entry) => {
                let (reference, list) = (entry.reference.clone(), entry.list.clone());
                Some(self.entry_face(&reference, &list, window, cx))
            }
        };
        let Some(face) = face else {
            return Vec::new();
        };
        let mut layer = vec![
            gpui::canvas(
                move |measured, window, _| {
                    if pane_bounds.replace(measured) != measured {
                        window.on_next_frame(|window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .into_any_element(),
        ];
        layer.extend(self.drawer(on, face, cx));
        layer.extend(grab.map(|grab| {
            let move_on = on.cloned();
            let release_on = on.cloned();
            let release = move |this: &mut Self,
                                _: &gpui::MouseUpEvent,
                                _: &mut Window,
                                cx: &mut Context<Self>| {
                if let Some(drawer) = &mut this.leaf_of_mut(release_on.as_ref()).drawer {
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
                        if let Some(drawer) = &mut this.leaf_of_mut(move_on.as_ref()).drawer {
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
                .on_mouse_up(gpui::MouseButton::Left, cx.listener(release.clone()))
                .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(release))
                .into_any_element()
        }));
        layer
    }

    fn drawer(
        &self,
        on: Option<&Member>,
        face: Face,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let drawer = self.leaf_of(on).drawer.as_ref()?;
        let theme = Theme::of(cx).clone();
        let expanded = drawer.size.expanded;
        let scroll = drawer.scroll.clone();
        let focus = drawer.focus.clone();
        let bounds = drawer.bounds.clone();
        let reveal = drawer.card().map(|card| card.reveal.clone());
        let (resize_on, close_on, escape_on, focus_on, expand_on, open_on) = (
            on.cloned(),
            on.cloned(),
            on.cloned(),
            on.cloned(),
            on.cloned(),
            on.cloned(),
        );
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
                .capture_any_mouse_down(cx.listener(move |this, _, window, cx| {
                    if let Some(on) = &focus_on {
                        this.focus_pane(on, window, cx);
                    }
                }))
                .on_mouse_down(gpui::MouseButton::Left, {
                    move |_, window, cx| {
                        if !focus.contains_focused(window, cx) {
                            window.focus(&focus, cx);
                        }
                        cx.stop_propagation();
                    }
                })
                .on_action(cx.listener(move |this, _: &CloseDrawer, window, cx| {
                    this.close_drawer(escape_on.as_ref(), window, cx);
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
                                if let Some(drawer) =
                                    &mut this.leaf_of_mut(resize_on.as_ref()).drawer
                                {
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
                            self.drawer_action(
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
                                    if let Some(drawer) =
                                        &mut this.leaf_of_mut(expand_on.as_ref()).drawer
                                    {
                                        drawer.size.expanded = !drawer.size.expanded;
                                        let expanded = drawer.size.expanded;
                                        if let Some(card) = drawer.card() {
                                            card.reveal.set(!expanded);
                                        }
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .children(face.open.map(|open| {
                            self.drawer_action(
                                "drawer-open",
                                icons::layout::Maximize,
                                "Open in pane",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.drop_drawer(open_on.as_ref(), cx);
                                    open(this, window, cx);
                                },
                            ))
                        }))
                        .child(
                            self.drawer_action(
                                "drawer-close",
                                icons::notifications::X,
                                "Close",
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_drawer(close_on.as_ref(), window, cx);
                                },
                            )),
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
                                window.on_next_frame(|window, _| window.refresh());
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

    /// What the drawer draws for an entry a link names: a session live or a
    /// run of its turns, an article's document, or a board's row.
    fn entry_face(
        &mut self,
        text: &str,
        list: &bezel::ui::list::VariableList<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Face {
        let theme = Theme::of(cx).clone();
        let named = match self.named(text, cx) {
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
        let title = self.label_of_row(&named.row, cx);
        let lead = vec![
            icons::icon(crate::view::search::kind_icon(named.kind))
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
                .child(format!("#{}", named.number))
                .into_any_element(),
        ];
        let (body, open): (AnyElement, OpenIn) = match (session, named.turns) {
            (Some(id), None) => (
                self.session_transcript(id, Some(list), window, cx),
                Rc::new(
                    move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                        this.select_session(id, window, cx)
                    },
                ),
            ),
            (Some(id), Some(turns)) => {
                let from = turns.from.saturating_sub(1) as usize;
                (
                    self.excerpt_body(id, turns, None, window, cx),
                    Rc::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            this.select_session(id, window, cx);
                            if let Some(chat) = this.workspace.read(cx).session(id) {
                                chat.transcript.reveal_turn(from);
                            }
                        },
                    ),
                )
            }
            (None, _) => {
                let row = named.row.clone();
                let article = match &row {
                    Row::Entry {
                        showing: Showing::Article(_),
                        ..
                    } => self.located(&row, cx).filter(|&(project, at)| {
                        self.workspace
                            .read(cx)
                            .article_in(project, at)
                            .is_some_and(|article| !self.article_on_screen(&article.path, cx))
                    }),
                    _ => None,
                };
                let body = article
                    .and_then(|(project, at)| self.article_peek(project, at, cx))
                    .unwrap_or_else(|| self.entry_row(named, cx));
                (
                    body,
                    Rc::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            this.open_row(&row, window, cx)
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
