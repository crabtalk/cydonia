//! Back and forward through the entries the window has been on: one history
//! for the window, across projects and spaces.
//!
//! It follows the entry in focus, not how it got there — a row in the sidebar,
//! a search hit, a link in an article, a press on another pane of a space.
//! Going back is going to that entry, wherever it is shown now, scrolled to
//! where it was left.

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
        root::{Cydonia, GoBack, GoForward},
        sidebar::Row,
    },
};

/// Entries kept behind and ahead, at most.
const KEPT: usize = 100;

/// Where an entry was left: the entry, and how far into it.
#[derive(Clone)]
struct Stop {
    member: Member,
    at: Option<Position>,
}

/// How far into an entry, by kind.
#[derive(Clone, Copy)]
enum Position {
    Transcript(ListOffset),
    Article(Point<Pixels>),
    Board {
        across: Point<Pixels>,
        down: Point<Pixels>,
    },
}

#[derive(Default)]
pub(crate) struct History {
    back: Vec<Stop>,
    forward: Vec<Stop>,
    /// The entry in focus when the window last looked.
    seen: Option<Member>,
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
    /// The entry in focus: the focused pane's in a space, the single pane's
    /// otherwise.
    fn focused_member(&self, cx: &Context<Self>) -> Option<Member> {
        self.leaf().entry.clone().or_else(|| self.lone_member(cx))
    }

    /// Note the entry in focus, and step the history when it changed. Called
    /// on every render, which every change of focus ends in.
    pub(crate) fn track_history(&mut self, cx: &Context<Self>) {
        let now = self.focused_member(cx);
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

    /// `member` as it is now, with how far into it it is scrolled.
    fn stop_at(&self, member: Member, cx: &Context<Self>) -> Stop {
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
        Stop { member, at }
    }

    /// Put the entry `stop` names back where it was left.
    fn restore(&self, stop: &Stop, cx: &Context<Self>) {
        let Some(at) = stop.at else {
            return;
        };
        let workspace = self.workspace.read(cx);
        let Some((project, showing)) = workspace.showing_of(&stop.member) else {
            return;
        };
        let Some(open) = workspace.projects.get(project) else {
            return;
        };
        match (showing, at) {
            (Showing::Session(id), Position::Transcript(top)) => {
                if let Some(chat) = open.session(id) {
                    chat.transcript.scroll_to_top(top);
                }
            }
            (Showing::Article(id), Position::Article(offset)) => {
                if let Some(ix) = open.article_ix(&id) {
                    open.articles[ix].scroll.set_offset(offset);
                }
            }
            (Showing::Board(id), Position::Board { across, down }) => {
                let scroll = self.boards.of(&id);
                scroll.across.set_offset(across);
                scroll.down.set_offset(down);
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

    /// Go to the nearest entry `back` (or forward) that still exists, moving
    /// the one in focus onto the other side. Entries gone since are dropped.
    fn step_history(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let next = match back {
                true => self.history.back.pop(),
                false => self.history.forward.pop(),
            };
            let Some(next) = next else {
                return;
            };
            let Some(row) = self.row_of(&next.member, cx) else {
                continue;
            };
            if let Some(left) = self.history.seen.clone() {
                let stop = self.stop_at(left, cx);
                match back {
                    true => self.history.forward.push(stop),
                    false => self.history.back.push(stop),
                }
            }
            self.history.stepping = true;
            self.open_row(&row, window, cx);
            self.restore(&next, cx);
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
