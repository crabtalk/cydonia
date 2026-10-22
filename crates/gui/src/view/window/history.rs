//! Back and forward through the entries the window has been on, and the
//! library: one history for the window, across projects and spaces.
//!
//! It follows the entry in focus, not how it got there — a row in the sidebar,
//! a search hit, a link in an article, a press on another pane of a space.
//! Going back is going to that entry, wherever it is shown now, scrolled to
//! where it was left; or to the library, narrowed as it was left.

use crate::model::workspace::{Member, Opening};
use bezel::{
    gpui::{Context, Div, IntoElement, ListOffset, Pixels, Point, Window, div, prelude::*},
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons},
    },
};

use crate::{
    model::workspace::Showing,
    view::{
        library::Shelf,
        sidebar::Row,
        window::root::{Cydonia, GoBack, GoForward},
    },
};

/// Entries kept behind and ahead, at most.
const KEPT: usize = 100;

/// What the window was on.
#[derive(Clone, PartialEq)]
enum Place {
    Entry(Member),
    Library,
}

/// Where the window was left: what it was on, and how far into it.
#[derive(Clone)]
struct Stop {
    place: Place,
    at: Option<Position>,
}

/// How far into an entry, by kind, or what the library was narrowed to.
#[derive(Clone)]
enum Position {
    Transcript(ListOffset),
    Article(Point<Pixels>),
    Board {
        across: Point<Pixels>,
        down: Point<Pixels>,
    },
    Library(Shelf),
}

#[derive(Default)]
pub(crate) struct History {
    back: Vec<Stop>,
    forward: Vec<Stop>,
    /// What the window was on when it last looked.
    seen: Option<Place>,
    /// Set while back or forward is moving the focus, so the move is not
    /// recorded as a new step.
    stepping: bool,
}

impl History {
    pub(crate) fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

impl Cydonia {
    /// What the window is on: the library while it is up, else the focused
    /// tab's entry.
    fn focused_place(&self, cx: &Context<Self>) -> Option<Place> {
        if self.library.is_some() {
            return Some(Place::Library);
        }
        self.focused_entry(cx).map(Place::Entry)
    }

    /// Note what the window is on, and step the history when it changed.
    /// Called on every render, which every change of focus ends in.
    pub(crate) fn track_history(&mut self, cx: &Context<Self>) {
        let now = self.focused_place(cx);
        let history = &mut self.history;
        // Spent on the first look after a step, whether or not it moved.
        let stepping = std::mem::take(&mut history.stepping);
        if now == history.seen {
            return;
        }
        let left = history.seen.take().filter(|_| !stepping);
        self.history.seen = now;
        if let Some(left) = left {
            let stop = self.stop_at(left, cx);
            let history = &mut self.history;
            history.back.push(stop);
            if history.back.len() > KEPT {
                history.back.remove(0);
            }
            history.forward.clear();
        }
    }

    /// `place` as it is now: how far into an entry it is scrolled, or what the
    /// library is narrowed to.
    fn stop_at(&self, place: Place, cx: &Context<Self>) -> Stop {
        let member = match place {
            Place::Entry(member) => member,
            Place::Library => {
                let at = self
                    .library
                    .as_ref()
                    .map(|library| Position::Library(library.read(cx).shelf(cx)));
                return Stop {
                    place: Place::Library,
                    at,
                };
            }
        };
        let workspace = self.workspace.read(cx);
        let at = workspace.showing_of(&member, cx).and_then(|(_, showing)| {
            Some(match showing {
                Showing::Session(_) => {
                    Position::Transcript(self.session_scroll(&member, cx)?.read(cx).state.top())
                }
                Showing::Article(_) => {
                    Position::Article(self.article_scroll(member.entry()?, cx)?.offset())
                }
                Showing::Board(_) => {
                    let scroll = self.board_scroll(member.entry()?, cx)?;
                    Position::Board {
                        across: scroll.across.offset(),
                        down: scroll.down.offset(),
                    }
                }
                Showing::Table(_) => return None,
            })
        });
        Stop {
            place: Place::Entry(member),
            at,
        }
    }

    /// Put the entry `stop` names back where it was left.
    fn restore(&self, stop: &Stop, cx: &Context<Self>) {
        let (Place::Entry(member), Some(at)) = (&stop.place, &stop.at) else {
            return;
        };
        let workspace = self.workspace.read(cx);
        let Some((_, showing)) = workspace.showing_of(member, cx) else {
            return;
        };
        match (showing, at) {
            (Showing::Session(_), Position::Transcript(top)) => {
                if let Some(view) = self.session_scroll(member, cx) {
                    view.read(cx).state.scroll_to_top(*top);
                }
            }
            (Showing::Article(_), Position::Article(offset)) => {
                if let Some(scroll) = member
                    .entry()
                    .and_then(|entry| self.article_scroll(entry, cx))
                {
                    scroll.set_offset(*offset);
                }
            }
            (Showing::Board(_), Position::Board { across, down }) => {
                if let Some(scroll) = member
                    .entry()
                    .and_then(|entry| self.board_scroll(entry, cx))
                {
                    scroll.across.set_offset(*across);
                    scroll.down.set_offset(*down);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        self.step_history(true, window, cx);
    }

    pub(crate) fn go_forward(
        &mut self,
        _: &GoForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_history(false, window, cx);
    }

    /// Go to the nearest place `back` (or forward) that still exists, moving
    /// the one the window is on onto the other side. Entries gone since are
    /// dropped.
    fn step_history(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let next = match back {
                true => self.history.back.pop(),
                false => self.history.forward.pop(),
            };
            let Some(next) = next else {
                return;
            };
            let row = match &next.place {
                Place::Entry(member) => match self.row_of(member, cx) {
                    Some(row) => Some(row),
                    None => continue,
                },
                Place::Library => None,
            };
            if let Some(left) = self.history.seen.clone() {
                let stop = self.stop_at(left, cx);
                match back {
                    true => self.history.forward.push(stop),
                    false => self.history.back.push(stop),
                }
            }
            self.history.stepping = true;
            match (row, next.at.clone()) {
                (Some(row), _) => {
                    self.open_row(&row, Opening::Preview, window, cx);
                    self.restore(&next, cx);
                }
                (None, Some(Position::Library(shelf))) => self.open_library(shelf, window, cx),
                (None, _) if self.library.is_none() => self.toggle_library(window, cx),
                (None, _) => {}
            }
            cx.notify();
            return;
        }
    }

    /// Back and forward, beside the sidebar's fold wherever that stands. One
    /// with nowhere to go is faded, and pressing it does nothing.
    fn history_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let button = |id: &'static str, glyph: &'static [u8], label: &'static str, open: bool| {
            theme
                .icon_button(
                    glyph,
                    ButtonStyle::Ghost,
                    Some(Fade::new(Painter::of(cx), id)),
                )
                .id(id)
                .flex_none()
                .when(!open, |button| button.opacity(0.4))
                .tooltip(move |window, cx| Tooltip::text(label, window, cx))
        };
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .child(
                button(
                    "history-back",
                    icons::arrows::ArrowLeft,
                    "Back",
                    self.history.can_go_back(),
                )
                .on_click(cx.listener(|this, _, window, cx| this.go_back(&GoBack, window, cx))),
            )
            .child(
                button(
                    "history-forward",
                    icons::arrows::ArrowRight,
                    "Forward",
                    self.history.can_go_forward(),
                )
                .on_click(
                    cx.listener(|this, _, window, cx| this.go_forward(&GoForward, window, cx)),
                ),
            )
    }

    /// The sidebar row that opens `member`, while it still exists.
    fn row_of(&self, member: &Member, cx: &Context<Self>) -> Option<Row> {
        let workspace = self.workspace.read(cx);
        let (project, showing) = workspace.showing_of(member, cx)?;
        Some(Row::Entry {
            project: workspace.projects.get(project)?.id,
            showing,
        })
    }

    /// The control that folds the sidebar away and brings it back.
    fn fold_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let label = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        // The same glyph either way: it names the column the button acts on,
        // and the tooltip says which way it will go. A glyph that flips is a
        // second thing to read for what the label already says.
        theme
            .icon_button(
                icons::layout::PanelLeft,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), "toggle-sidebar")),
            )
            .id("toggle-sidebar")
            .flex_none()
            .tooltip(move |window, cx| Tooltip::text(label, window, cx))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    /// The fold with back and forward beside it. Every band along the
    /// window's left edge places this one element, so the three stand in the
    /// same spots whether the sidebar is open or folded.
    pub(crate) fn fold_controls(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .child(self.fold_toggle(cx))
            .child(self.history_buttons(cx))
    }
}
