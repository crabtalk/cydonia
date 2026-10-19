//! The workspace search: a query applied from the palette — see
//! [`crate::view::modals::palette`] — narrows the sidebar, the boards and the
//! panes' find bars until the sidebar's clear button is pressed.
//!
//! A query is searched off disk on a background thread — see
//! [`artifact::search::disk`] — so an entry's hits are as of its last write.

use crate::model::workspace::Workspace;
use crate::view::{modals::palette::Hit, sidebar::Row};
use artifact::{
    kind::Kind,
    project::fs,
    search::{self, Block, Query},
};
use bezel::gpui::{App, Context, Entity, Task};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

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

/// The query applied to the workspace, while there is one. One entity for the
/// window: the sidebar draws its rows by it, and the window hands it to every
/// pane's find.
pub(crate) struct Search {
    workspace: Entity<Workspace>,
    applied: Option<Applied>,
    task: Option<Task<()>>,
}

impl Search {
    pub(crate) fn new(workspace: Entity<Workspace>) -> Self {
        Self {
            workspace,
            applied: None,
            task: None,
        }
    }

    /// What the palette opens holding: the applied query and its kind.
    pub(crate) fn draft(&self) -> (String, Option<Kind>) {
        self.applied
            .as_ref()
            .map_or_else(Default::default, |applied| {
                (applied.query.text().to_owned(), applied.filter)
            })
    }

    /// Apply `query`, narrowed to `kind`, to the workspace.
    pub(crate) fn apply(&mut self, query: Query, kind: Option<Kind>, cx: &mut Context<Self>) {
        self.applied = Some(Applied {
            query,
            filter: kind,
            found: Vec::new(),
            ready: false,
        });
        self.refresh(cx);
        cx.notify();
    }

    /// Search the open projects for the applied query again.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(applied) = self.applied.as_mut() else {
            return;
        };
        let query = applied.query.clone();
        let roots = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .filter_map(|open| open.location().local().map(Path::to_path_buf))
            .collect::<Vec<_>>();
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(applied) = this.applied.as_mut() {
                    applied.found = found;
                    applied.ready = true;
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn query(&self) -> Option<&Query> {
        self.applied.as_ref().map(|a| &a.query)
    }

    /// The rows the applied query leaves standing, most relevant first.
    pub(crate) fn rows(&self, cx: &App) -> Option<Vec<Row>> {
        let applied = self.applied.as_ref()?;
        Some(
            Hit::ranked(applied.found.clone(), self.workspace.read(cx), cx)
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

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.applied = None;
        self.task = None;
        cx.notify();
    }

    /// The applied query as the sidebar shows it: its text, its kind, and
    /// whether its search has come back.
    pub(crate) fn shown(&self) -> Option<(String, Option<Kind>, bool)> {
        let applied = self.applied.as_ref()?;
        Some((
            applied.query.text().to_owned(),
            applied.filter,
            applied.ready,
        ))
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
