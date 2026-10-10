//! Back and forward through the entries the window has been on, and the
//! library: one history for the window, across projects and spaces.
//!
//! It follows the entry in focus, not how it got there — a row in the sidebar,
//! a search hit, a link in an article, a press on another pane of a space.
//! Going back is going to that entry, wherever it is shown now, scrolled to
//! where it was left; or to the library, narrowed as it was left.

use artifact::space::Member;
use bezel::{
    gpui::{Context, IntoElement, ListOffset, Pixels, Point, Window, div, prelude::*},
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
        root::{Cydonia, GoBack, GoForward},
        sidebar::Row,
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
    /// What the window is on: the library while it is up, else the entry in
    /// focus — the focused pane's in a space, the single pane's otherwise.
    fn focused_place(&self, cx: &Context<Self>) -> Option<Place> {
        if self.library.is_some() {
            return Some(Place::Library);
        }
        self.leaf()
            .entry
            .clone()
            .or_else(|| self.lone_member(cx))
            .map(Place::Entry)
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
                    .map(|library| Position::Library(library.shelf(cx)));
                return Stop {
                    place: Place::Library,
                    at,
                };
            }
        };
        let workspace = self.workspace.read(cx);
        let at = workspace
            .showing_of(&member)
            .and_then(|(project, showing)| {
                let open = workspace.projects.get(project)?;
                Some(match showing {
                    Showing::Session(id) => {
                        Position::Transcript(open.session(id)?.transcript.top())
                    }
                    Showing::Article(id) => {
                        Position::Article(open.articles[open.article_ix(&id)?].scroll.offset())
                    }
                    Showing::Board(id) => {
                        let scroll = self.boards.of(&id);
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
        let Some((project, showing)) = workspace.showing_of(member) else {
            return;
        };
        let Some(open) = workspace.projects.get(project) else {
            return;
        };
        match (showing, at) {
            (Showing::Session(id), Position::Transcript(top)) => {
                if let Some(chat) = open.session(id) {
                    chat.transcript.scroll_to_top(*top);
                }
            }
            (Showing::Article(id), Position::Article(offset)) => {
                if let Some(ix) = open.article_ix(&id) {
                    open.articles[ix].scroll.set_offset(*offset);
                }
            }
            (Showing::Board(id), Position::Board { across, down }) => {
                let scroll = self.boards.of(&id);
                scroll.across.set_offset(*across);
                scroll.down.set_offset(*down);
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
                    self.open_row(&row, window, cx);
                    self.restore(&next, cx);
                }
                (None, Some(Position::Library(shelf))) => self.open_library(shelf, cx),
                (None, _) if self.library.is_none() => self.toggle_library(cx),
                (None, _) => {}
            }
            cx.notify();
            return;
        }
    }

    /// Back and forward, beside the sidebar's fold wherever that stands. One
    /// with nowhere to go is faded, and pressing it does nothing.
    pub(crate) fn history_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
        let (project, showing) = workspace.showing_of(member)?;
        Some(Row::Entry {
            project: workspace.projects.get(project)?.path.clone(),
            showing,
        })
    }
}
