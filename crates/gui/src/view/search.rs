//! The workspace search: a query applied from the palette — see
//! [`crate::view::modals::palette`] — narrows the sidebar, the boards and the
//! panes' find bars until the sidebar's clear button is pressed.
//!
//! A query is searched off disk on a background thread — see
//! [`artifact::search::disk`] — so an entry's hits are as of its last write.

use crate::view::{
    keymap::{self, Command},
    modals::palette::{Filter, Hit, ToggleSearch},
    sidebar::{self, Row},
    window::root::Cydonia,
};
use artifact::{
    kind::Kind,
    project::fs,
    search::{self, Block, Query},
};
use bezel::{
    gpui::{AnyElement, App, Context, Task, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset as _},
    ui::{icons, popover, widgets::Buttons as _},
};
use std::{collections::HashMap, path::PathBuf, sync::mpsc, time::Duration};

/// How long typing has to pause before the query is searched.
pub(crate) const SETTLE: Duration = Duration::from_millis(80);

/// An entry's best match as the search found it.
#[derive(Clone)]
pub(crate) struct Found {
    pub(crate) root: PathBuf,
    pub(crate) kind: Kind,
    pub(crate) id: String,
    pub(crate) in_title: bool,
    pub(crate) snippet: Option<(String, std::ops::Range<usize>)>,
}

struct Applied {
    query: Query,
    filter: Option<Kind>,
    found: Vec<Found>,
    ready: bool,
}

/// The query applied to the workspace, while there is one.
#[derive(Default)]
pub(crate) struct Search {
    applied: Option<Applied>,
    applied_task: Option<Task<()>>,
}

impl Cydonia {
    /// What the palette opens holding: the applied query and its kind.
    pub(crate) fn applied_draft(&self) -> (String, Option<Kind>) {
        self.search
            .applied
            .as_ref()
            .map_or_else(Default::default, |applied| {
                (applied.query.text().to_owned(), applied.filter)
            })
    }

    /// Apply `query`, narrowed to `kind`, to the workspace.
    pub(crate) fn apply_search(
        &mut self,
        query: Query,
        kind: Option<Kind>,
        cx: &mut Context<Self>,
    ) {
        self.search.applied = Some(Applied {
            query,
            filter: kind,
            found: Vec::new(),
            ready: false,
        });
        for leaf in &mut self.leaves {
            leaf.finding = false;
            leaf.find_at = 0;
        }
        self.refresh_applied_search(cx);
        cx.notify();
    }

    pub(crate) fn refresh_applied_search(&mut self, cx: &mut Context<Self>) {
        let Some(applied) = self.search.applied.as_mut() else {
            return;
        };
        let query = applied.query.clone();
        let roots = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|p| p.path.clone())
            .collect::<Vec<_>>();
        self.search.applied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(applied) = this.search.applied.as_mut() {
                    applied.found = found;
                    applied.ready = true;
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn applied_query(&self) -> Option<&Query> {
        self.search.applied.as_ref().map(|a| &a.query)
    }

    pub(crate) fn applied_rows(&self, cx: &App) -> Option<Vec<Row>> {
        let applied = self.search.applied.as_ref()?;
        Some(
            Hit::ranked(applied.found.clone(), self.workspace.read(cx))
                .into_iter()
                .filter(|hit| {
                    applied
                        .filter
                        .is_none_or(|kind| hit.row.kind() == Some(kind))
                })
                .map(|hit| hit.row)
                .collect(),
        )
    }

    pub(crate) fn clear_applied_search(&mut self, cx: &mut Context<Self>) {
        self.search.applied = None;
        self.search.applied_task = None;
        cx.notify();
    }

    /// The sidebar's row that raises the palette.
    pub(crate) fn search_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let chord = keymap::label(Command::Search, &self.workspace.read(cx).settings.shortcuts);
        if let Some(applied) = &self.search.applied {
            let label = match applied.filter {
                Some(kind) => format!("{} · {}", applied.query.text(), Filter::Kind(kind).label()),
                None => applied.query.text().to_owned(),
            };
            let count = self.applied_rows(cx).map_or(0, |rows| rows.len());
            return sidebar::row("applied-search", "applied-search", false, false, 0, &theme)
                .flex_none()
                .min_w_0()
                .mb(px(4.))
                .child(
                    icons::icon(icons::text::Search)
                        .size(px(14.))
                        .text_color(theme.text_muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::Body)
                        .text_color(theme.text)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .child(if !applied.ready {
                            "…".to_owned()
                        } else {
                            count.to_string()
                        }),
                )
                .child(
                    theme
                        .ghost("clear-workspace-search")
                        .debug_selector(|| "clear-workspace-search".into())
                        .flex_none()
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(14.))
                                .text_color(theme.text_muted),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.clear_applied_search(cx);
                        })),
                )
                .on_click(
                    cx.listener(|this, _, window, cx| {
                        this.toggle_search(&ToggleSearch, window, cx)
                    }),
                )
                .into_any_element();
        }
        sidebar::row("search-row", "search-row", false, false, 0, &theme)
            .flex_none()
            .mb(px(4.))
            .child(
                icons::icon(icons::text::Search)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .text_style(TextStyle::Body)
                    .text_color(theme.text_muted)
                    .child("Search"),
            )
            .children(chord.map(|chord| popover::kbd_hint(&theme, chord)))
            .on_click(
                cx.listener(|this, _, window, cx| this.toggle_search(&ToggleSearch, window, cx)),
            )
            .into_any_element()
    }
}

/// Every entry in `roots` holding `query`, each once with its best match: the
/// title where it matched there, else its earliest block.
pub(crate) fn search_all(roots: &[PathBuf], query: &Query) -> Vec<Found> {
    let kinds = [Kind::Article, Kind::Board, Kind::Session];
    let mut found = Vec::new();
    for root in roots {
        let (tx, rx) = mpsc::channel();
        search::disk(&fs::Project::new(root), &kinds, query, &tx);
        drop(tx);
        let mut best: HashMap<(Kind, String), search::Match> = HashMap::new();
        for hit in rx {
            let key = (hit.item.kind, hit.item.id.clone());
            if best
                .get(&key)
                .is_none_or(|held| rank(&hit.block) < rank(&held.block))
            {
                best.insert(key, hit);
            }
        }
        found.extend(best.into_iter().map(|((kind, id), hit)| {
            let in_title = hit.block == Block::Title;
            Found {
                root: root.clone(),
                kind,
                id,
                in_title,
                snippet: (!in_title).then_some((hit.line, hit.column)),
            }
        }));
    }
    found
}

/// Which of two matches in one entry is shown: the title, then the earliest
/// block.
fn rank(block: &Block) -> (u8, usize) {
    match block {
        Block::Title => (0, 0),
        Block::Line(ix) | Block::Chat(ix) => (1, *ix),
        Block::Card(_) => (1, usize::MAX),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/workspace_search.rs"]
mod tests;
