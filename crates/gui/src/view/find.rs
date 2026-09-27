//! Finding text in the document a pane shows: an article, or a transcript.
//!
//! The board's find narrows its cards instead — see
//! [`crate::view::board::FindCard`]. Both share the pane's find field and
//! [`crate::view::leaf::Leaf::finding`].
//!
//! Hits are looked for in the rendered text of each block, the text a reader
//! sees, not in the markdown source.

use crate::view::{
    board::{self, DismissFind},
    component::transcript,
    leaf::Pane,
    root::Cydonia,
};
use artifact::{
    search::Query,
    space::{Kind, Member},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, KeyBinding, SharedString, Window, actions, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{icons, input::TextField, tooltip::Tooltip, widgets::Buttons as _},
};
use editor::{Anchor, AnchorId};
use markdown::{Annotation, Cursor, Doc, Selection};

actions!(cydonia_find, [FindNext, FindPrev]);

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(board::FIND_CONTEXT);
    vec![
        KeyBinding::new("enter", FindNext, ctx),
        KeyBinding::new("shift-enter", FindPrev, ctx),
    ]
}

/// Every match of `query` in `doc`, in document order.
pub(crate) fn hits(doc: &Doc, query: &Query) -> Vec<Selection> {
    let mut hits = Vec::new();
    for (ix, block) in doc.blocks.iter().enumerate() {
        for part in block.parts() {
            let Some(text) = block.text_at(part) else {
                continue;
            };
            if !query.is_match(text.text.as_bytes()) {
                continue;
            }
            hits.extend(query.find(&text.text).map(|range| {
                Selection::new(
                    Cursor::new(ix, part, range.start),
                    Cursor::new(ix, part, range.end),
                )
            }));
        }
    }
    hits
}

/// `hits` as washes, the one at `current` set apart.
pub(crate) fn annotations(
    hits: &[Selection],
    current: Option<Selection>,
) -> Vec<(Selection, Annotation)> {
    hits.iter()
        .map(|hit| {
            let state = match Some(*hit) == current {
                true => Annotation::Current,
                false => Annotation::Match,
            };
            (*hit, state)
        })
        .collect()
}

/// Where a pane's find points: every hit, and which of them is current.
enum Found {
    Article {
        editor: Entity<editor::Editor>,
        hits: Vec<Selection>,
    },
    Transcript {
        session: u64,
        hits: Vec<(usize, Selection)>,
    },
}

impl Found {
    fn len(&self) -> usize {
        match self {
            Self::Article { hits, .. } => hits.len(),
            Self::Transcript { hits, .. } => hits.len(),
        }
    }
}

impl Cydonia {
    /// The query a pane finds by, while its bar is up and holds one.
    fn text_query(&self, on: Option<&Member>, cx: &App) -> Option<Query> {
        let leaf = self.leaf_of(on);
        if leaf.finding {
            Query::literal(leaf.find_field.read(cx).content())
        } else {
            self.applied_query().cloned()
        }
    }

    fn found(&self, on: Option<&Member>, cx: &App) -> Option<Found> {
        let query = self.text_query(on, cx)?;
        let workspace = self.workspace.read(cx);
        // A pane in a space says what it shows by its member; a window
        // showing one entry, by its leaf.
        let pane = match on.map(|member| member.kind) {
            Some(Kind::Session) => Pane::Chat,
            Some(Kind::Board) => Pane::Board,
            Some(Kind::Article) => Pane::Article,
            Some(Kind::Table) => Pane::Table,
            None => self.leaf_of(on).pane,
        };
        match pane {
            Pane::Article => {
                let editor = workspace.article_of(on)?.editor.clone()?;
                let hits = hits(editor.read(cx).doc(), &query);
                Some(Found::Article { editor, hits })
            }
            Pane::Chat => {
                let session = workspace.session_id_of(on)?;
                let chat = workspace.session(session)?;
                let hits = transcript::hits(&chat.items, &query);
                Some(Found::Transcript { session, hits })
            }
            Pane::Board | Pane::Table => None,
        }
    }

    /// Wash the article's hits in its editor. Run on every paint of the pane,
    /// so hits follow edits; the anchors are written only when they differ.
    /// The editor's anchors hold nothing else.
    pub(crate) fn paint_article_find(
        &self,
        editor: &Entity<editor::Editor>,
        on: Option<&Member>,
        cx: &mut App,
    ) {
        let anchors: Vec<Anchor> = match self.text_query(on, cx) {
            Some(query) => {
                let hits = hits(editor.read(cx).doc(), &query);
                let current = current(self.leaf_of(on).find_at, hits.len());
                hits.iter()
                    .enumerate()
                    .map(|(ix, hit)| Anchor {
                        id: AnchorId(ix as u64),
                        range: *hit,
                        state: match Some(ix) == current {
                            true => Annotation::Current,
                            false => Annotation::Match,
                        },
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        if editor.read(cx).anchors() != anchors.as_slice() {
            editor.update(cx, |editor, cx| editor.set_anchors(anchors, cx));
        }
    }

    /// The query changed: back to the first hit, and show it.
    pub(crate) fn find_changed(&mut self, field: &Entity<TextField>, cx: &mut Context<Self>) {
        let Some(leaf) = self
            .leaves
            .iter()
            .position(|leaf| &leaf.find_field == field)
        else {
            return;
        };
        let on = self.leaves[leaf].entry.clone();
        self.leaves[leaf].find_at = 0;
        self.reveal_find(on.as_ref(), cx);
        cx.notify();
    }

    fn find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_find(1, cx);
    }

    fn find_prev(&mut self, _: &FindPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step_find(-1, cx);
    }

    fn step_find(&mut self, by: isize, cx: &mut Context<Self>) {
        let on = self.leaf().entry.clone();
        let Some(len) = self.found(on.as_ref(), cx).map(|found| found.len()) else {
            return;
        };
        if len == 0 {
            return;
        }
        let at = current(self.leaf().find_at, len).unwrap_or(0) as isize;
        self.leaf_mut().find_at = (at + by).rem_euclid(len as isize) as usize;
        self.reveal_find(on.as_ref(), cx);
        cx.notify();
    }

    /// Opening a filtered entry starts at its first match, not its saved viewport.
    pub(crate) fn reveal_applied_match(&mut self, cx: &mut Context<Self>) {
        if self.applied_query().is_none() {
            return;
        }
        self.leaf_mut().finding = false;
        self.leaf_mut().find_at = 0;
        let on = self.leaf().entry.clone();
        if self.leaf().pane == Pane::Board {
            self.reveal_board_match(on.as_ref(), cx);
        } else {
            self.reveal_find(on.as_ref(), cx);
        }
        cx.notify();
    }

    /// Bring the current hit into view and mark it.
    fn reveal_find(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        let at = self.leaf_of(on).find_at;
        match self.found(on, cx) {
            Some(Found::Article { editor, hits }) => {
                if let Some(hit) = current(at, hits.len()).map(|ix| hits[ix]) {
                    editor.update(cx, |editor, cx| editor.select(hit, cx));
                }
            }
            Some(Found::Transcript { session, hits }) => {
                let hit = current(at, hits.len()).map(|ix| hits[ix]);
                self.workspace.update(cx, |workspace, cx| {
                    workspace.with_session(session, cx, |chat| {
                        chat.transcript.show_find(&chat.items, hit);
                    });
                });
            }
            None => {}
        }
    }

    /// Take a transcript's find off with the bar.
    pub(crate) fn clear_text_find(&mut self, on: Option<&Member>, cx: &mut Context<Self>) {
        let session = self.workspace.read(cx).session_id_of(on);
        if let Some(session) = session {
            self.workspace.update(cx, |workspace, cx| {
                workspace.with_session(session, cx, |chat| {
                    chat.transcript.show_find(&chat.items, None)
                });
            });
        }
    }

    /// The query a transcript washes its hits with, handed over on every
    /// paint of the pane.
    pub(crate) fn transcript_query(&self, on: Option<&Member>, cx: &App) -> Option<Query> {
        self.text_query(on, cx)
    }

    /// The bar over an article or a transcript: the field, how many hits and
    /// which, and the ways between them.
    pub(crate) fn search_pill(
        &self,
        on: Option<&Member>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let leaf = self.leaf_of(on);
        if !leaf.finding && self.applied_query().is_none() {
            return None;
        }
        let theme = Theme::of(cx).clone();
        let board = on.map_or(leaf.pane == Pane::Board, |member| {
            member.kind == Kind::Board
        });
        let count: Option<SharedString> = if board {
            let query = self.board_query(on, cx);
            self.workspace.read(cx).board_of(on).map(|board| {
                let count = board
                    .columns
                    .iter()
                    .flat_map(|column| &column.cards)
                    .filter(|card| {
                        board::card_matches(card, board.handle_of(card).as_deref(), &query)
                    })
                    .count();
                format!("{count} cards").into()
            })
        } else {
            self.found(on, cx)
                .map(|found| match current(leaf.find_at, found.len()) {
                    Some(at) => format!("{} of {}", at + 1, found.len()).into(),
                    None => "no results".into(),
                })
        };
        let step =
            |id: &'static str, icon, tip: &'static str, by: isize, cx: &mut Context<Self>| {
                theme
                    .ghost(id)
                    .p(px(4.))
                    .rounded_full()
                    .child(icons::icon(icon).size(px(12.)).text_color(theme.text_faint))
                    .tooltip(move |window, cx| Tooltip::text(tip, window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.step_find(by, cx);
                    }))
            };
        Some(
            div()
                .id("search-pill")
                .debug_selector(|| "search-pill".into())
                .absolute()
                .top(px(12.))
                .right(px(16.))
                .w(px(240.))
                .h(px(26.))
                .text_style(TextStyle::Body)
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(2.))
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface_raised)
                .on_action(cx.listener(Self::find_next))
                .on_action(cx.listener(Self::find_prev))
                .on_action(cx.listener(|this, _: &DismissFind, window, cx| {
                    this.dismiss_text_find(window, cx);
                }))
                .child(
                    icons::icon(icons::text::Search)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .when(leaf.finding, |el| el.child(leaf.find_field.clone()))
                        .when(!leaf.finding, |el| {
                            el.truncate().child(
                                self.applied_query()
                                    .map(|q| q.text().to_owned())
                                    .unwrap_or_default(),
                            )
                        }),
                )
                .children(count.map(|count| {
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.text_faint)
                        .child(count)
                }))
                .children((!board).then(|| {
                    step(
                        "find-prev",
                        icons::arrows::ChevronUp,
                        "Previous match",
                        -1,
                        cx,
                    )
                }))
                .children(
                    (!board).then(|| {
                        step("find-next", icons::arrows::ChevronDown, "Next match", 1, cx)
                    }),
                )
                .child({
                    theme
                        .ghost("find-close")
                        .debug_selector(|| "find-close".into())
                        .flex_none()
                        .p(px(4.))
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .tooltip(|window, cx| Tooltip::text("Stop finding", window, cx))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_text_find(window, cx);
                        }))
                })
                .into_any_element(),
        )
    }

    fn dismiss_text_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.leaf().finding && self.applied_query().is_some() {
            self.clear_applied_search(cx);
            window.focus(&self.leaf().focus, cx);
            return;
        }
        let on = self.leaf().entry.clone();
        self.clear_text_find(on.as_ref(), cx);
        self.dismiss_find(&DismissFind, window, cx);
        window.focus(&self.leaf().focus, cx);
    }
}

/// Which of `len` hits `at` points at, wrapping; nothing when there are none.
fn current(at: usize, len: usize) -> Option<usize> {
    (len > 0).then(|| at % len)
}

#[cfg(test)]
#[path = "../../tests/unit/find.rs"]
mod tests;
