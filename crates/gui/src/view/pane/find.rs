//! Finding text in the document a pane shows: an article, or a transcript.
//!
//! The board's find narrows its cards instead — see
//! [`crate::view::board::FindCard`]. Both share the pane's find field and
//! [`crate::view::pane::Tab::finding`].
//!
//! Hits are looked for in the rendered text of each block, the text a reader
//! sees, not in the markdown source.

use super::Tab;
use crate::view::{
    board::{self, DismissFind, FindCard},
    window::{reach::InWindow as _, root::Cydonia},
};
use artifact::{kind::Kind, search::Query};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Focusable as _, KeyBinding, SharedString, Window, actions,
        div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{icons, tooltip::Tooltip, widgets::Buttons as _},
};
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

/// What a pane's find bar and the workspace search hand each thing the pane
/// holds — see [`Searchable`].
#[derive(Clone, Default)]
pub(crate) struct Find {
    /// The bar's text, while the bar is up.
    pub(crate) bar: Option<String>,
    /// Whether the bar reads its text as a regular expression.
    pub(crate) pattern: bool,
    /// The workspace search applied.
    pub(crate) applied: Option<Query>,
    /// Which hit is current, counted from the first.
    pub(crate) at: usize,
}

impl Find {
    /// What a text find looks for: the bar's query while the bar is up, the
    /// applied search otherwise.
    pub(crate) fn query(&self) -> Option<Query> {
        match &self.bar {
            Some(text) => match self.pattern {
                true => Query::pattern(text).ok().flatten(),
                false => Query::literal(text),
            },
            None => self.applied.clone(),
        }
    }

    /// What a board is narrowed by, as text: the bar's while it is up, the
    /// applied search's otherwise, and empty for none.
    pub(crate) fn text(&self) -> String {
        match &self.bar {
            Some(text) => text.clone(),
            None => self
                .applied
                .as_ref()
                .map(|query| query.text().to_owned())
                .unwrap_or_default(),
        }
    }

    /// Which of `len` hits is current, wrapping; nothing when there are none.
    pub(crate) fn current(&self, len: usize) -> Option<usize> {
        current(self.at, len)
    }
}

/// A thing a pane shows that the pane's find reaches: told the find whenever
/// it changes, asked how many hits it finds, and asked to bring the current
/// one into view.
pub(crate) trait Searchable: Sized + 'static {
    fn set_find(&mut self, find: Find, cx: &mut Context<Self>);
    fn hits(&self, cx: &App) -> usize;
    fn reveal_hit(&mut self, cx: &mut Context<Self>);
}

impl Tab {
    /// What the pane's find bar and the applied search say now.
    pub(crate) fn find(&self, cx: &App) -> Find {
        Find {
            bar: self
                .finding
                .then(|| self.find_field.read(cx).content().to_string()),
            pattern: self.find_pattern,
            applied: self.applied.clone(),
            at: self.find_at,
        }
    }

    /// Hand the pane's find to everything it holds.
    pub(crate) fn push_find(&mut self, cx: &mut Context<Self>) {
        let find = self.find(cx);
        for view in self.boards.clone() {
            view.update(cx, |view, cx| view.set_find(find.clone(), cx));
        }
        for view in self.articles.clone() {
            view.update(cx, |view, cx| view.set_find(find.clone(), cx));
        }
        for view in self.sessions.clone() {
            view.update(cx, |view, cx| view.set_find(find.clone(), cx));
        }
        cx.notify();
    }

    /// The workspace search applied moved.
    pub(crate) fn set_applied(&mut self, applied: Option<Query>, cx: &mut Context<Self>) {
        if applied.is_some() {
            self.finding = false;
            self.find_at = 0;
        }
        self.applied = applied;
        self.push_find(cx);
    }

    /// What the pane shows, as its find reaches it.
    fn shown_kind(&self) -> Kind {
        self.entry
            .as_ref()
            .map_or(self.kind, artifact::space::Member::kind)
    }

    /// How many hits the find finds in what the pane shows.
    fn hit_count(&self, cx: &App) -> Option<usize> {
        match self.shown_kind() {
            Kind::Article => Some(self.article_shown(cx)?.read(cx).hits(cx)),
            Kind::Board => Some(self.board_shown(cx)?.read(cx).hits(cx)),
            Kind::Session => Some(self.session_shown(cx)?.read(cx).hits(cx)),
            Kind::Table | Kind::App => None,
        }
    }

    /// Put the find bar up, take the caret back to a field already up, or —
    /// where the caret is in it already — put it away. The chord that raises a
    /// bar is the one a reader reaches for to be rid of it, and the bar is the
    /// query: dismissing it clears what the board is narrowed by.
    pub(crate) fn find_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field = self.find_field.clone();
        if self.finding && field.read(cx).focus_handle(cx).is_focused(window) {
            self.dismiss_find(cx);
            return;
        }
        self.finding = true;
        let placeholder = match self.kind {
            Kind::Board => "find a card…",
            _ => "find…",
        };
        field.update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        window.focus(&field.read(cx).focus_handle(cx), cx);
        self.push_find(cx);
    }

    /// Done finding: the field goes and takes its query with it.
    pub(crate) fn dismiss_find(&mut self, cx: &mut Context<Self>) {
        self.finding = false;
        self.find_field.update(cx, |field, cx| field.clear(cx));
        self.push_find(cx);
    }

    /// The query changed: back to the first hit, and show it.
    pub(crate) fn find_changed(&mut self, cx: &mut Context<Self>) {
        self.find_at = 0;
        self.push_find(cx);
        self.reveal_find(cx);
    }

    /// Whether the bar is up in pattern mode over a field that does not parse.
    fn bad_pattern(&self, cx: &App) -> bool {
        self.finding
            && self.find_pattern
            && Query::pattern(self.find_field.read(cx).content()).is_err()
    }

    fn toggle_find_pattern(&mut self, cx: &mut Context<Self>) {
        self.find_pattern = !self.find_pattern;
        self.find_at = 0;
        self.push_find(cx);
        self.reveal_find(cx);
    }

    pub(crate) fn step_find(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(len) = self.hit_count(cx) else {
            return;
        };
        if len == 0 {
            return;
        }
        let at = current(self.find_at, len).unwrap_or(0) as isize;
        self.find_at = (at + by).rem_euclid(len as isize) as usize;
        self.push_find(cx);
        self.reveal_find(cx);
    }

    /// Opening a filtered entry starts at its first match, not its saved viewport.
    pub(crate) fn reveal_applied_match(&mut self, cx: &mut Context<Self>) {
        if self.applied.is_none() {
            return;
        }
        self.finding = false;
        self.find_at = 0;
        self.push_find(cx);
        self.reveal_find(cx);
    }

    /// Bring the current hit into view and mark it.
    fn reveal_find(&mut self, cx: &mut Context<Self>) {
        match self.shown_kind() {
            Kind::Article => {
                if let Some(view) = self.article_shown(cx) {
                    view.update(cx, |view, cx| view.reveal_hit(cx));
                }
            }
            Kind::Board => {
                if let Some(view) = self.board_shown(cx) {
                    view.update(cx, |view, cx| view.reveal_hit(cx));
                }
            }
            Kind::Session => {
                if let Some(view) = self.session_shown(cx) {
                    view.update(cx, |view, cx| view.reveal_hit(cx));
                }
            }
            Kind::Table | Kind::App => {}
        }
    }

    /// The bar over an article or a transcript: the field, how many hits and
    /// which, and the ways between them.
    pub(crate) fn search_pill(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.finding && self.applied.is_none() {
            return None;
        }
        let theme = Theme::of(cx).clone();
        let board = self.shown_kind() == Kind::Board;
        let count: Option<SharedString> = if board {
            self.hit_count(cx)
                .map(|count| format!("{count} cards").into())
        } else if self.bad_pattern(cx) {
            Some("bad pattern".into())
        } else {
            self.hit_count(cx)
                .map(|len| match current(self.find_at, len) {
                    Some(at) => format!("{} of {}", at + 1, len).into(),
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
                .occlude()
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
                        .when(self.finding, |el| el.child(self.find_field.clone()))
                        .when(!self.finding, |el| {
                            el.truncate().child(
                                self.applied
                                    .as_ref()
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
                .children((self.finding && !board).then(|| {
                    let on = self.find_pattern;
                    theme
                        .ghost("find-pattern")
                        .flex_none()
                        .p(px(4.))
                        .rounded_full()
                        .when(on, |el| el.bg(theme.element_active))
                        .child(
                            icons::icon(icons::text::Regex)
                                .size(px(12.))
                                .text_color(match on {
                                    true => theme.text,
                                    false => theme.text_faint,
                                }),
                        )
                        .tooltip(|window, cx| Tooltip::text("Match a pattern", window, cx))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_find_pattern(cx);
                        }))
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
        if !self.finding && self.applied.is_some() {
            self.defer_window(window, cx, |root, _, cx| {
                root.search.update(cx, crate::view::search::Search::clear)
            });
            window.focus(&self.focus, cx);
            return;
        }
        self.dismiss_find(cx);
        window.focus(&self.focus, cx);
    }
}

impl Cydonia {
    /// Hand every pane the workspace search applied.
    pub(crate) fn push_finds(&mut self, cx: &mut Context<Self>) {
        let applied = self.search.read(cx).query().cloned();
        for pane in self.leaves.clone() {
            pane.update(cx, |pane, cx| pane.set_applied(applied.clone(), cx));
        }
    }

    /// Opening a filtered entry starts at its first match, not its saved
    /// viewport.
    pub(crate) fn reveal_applied_match(&mut self, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.reveal_applied_match(cx));
    }

    pub(crate) fn find_card(&mut self, _: &FindCard, window: &mut Window, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.find_card(window, cx));
    }

    pub(crate) fn find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.step_find(1, cx));
    }

    pub(crate) fn find_prev(&mut self, _: &FindPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.step_find(-1, cx));
    }
}

/// Which of `len` hits `at` points at, wrapping; nothing when there are none.
fn current(at: usize, len: usize) -> Option<usize> {
    (len > 0).then(|| at % len)
}

#[cfg(test)]
#[path = "../../../tests/unit/find.rs"]
mod tests;
