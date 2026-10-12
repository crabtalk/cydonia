//! Labels across every open project — see [`artifact::label`]. Each entry
//! holds its own; the set of labels is whatever the entries carry.

use super::{Showing, Workspace};
use bezel::gpui::{App, Context};
use std::collections::BTreeMap;

impl Workspace {
    /// The labels the entry `showing` carries in the project at `project`.
    /// `None` for an entry that cannot carry any, which is a table.
    pub fn labels_of<'a>(
        &'a self,
        project: usize,
        showing: &Showing,
        cx: &'a App,
    ) -> Option<&'a [String]> {
        let open = self.projects.get(project)?;
        match showing {
            Showing::Session(id) => open.session(*id, cx).map(|chat| chat.labels.as_slice()),
            Showing::Article(id) => open
                .article_ix(id)
                .map(|ix| open.articles[ix].labels.as_slice()),
            Showing::Board(id) => open
                .board_ix(id)
                .map(|ix| open.boards[ix].board.read(cx).labels.as_slice()),
            Showing::Table(_) => None,
        }
    }

    /// Give the entry exactly `labels`, normalised.
    pub fn set_labels(
        &mut self,
        project: usize,
        showing: &Showing,
        labels: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let labels = artifact::label::normalize_all(labels);
        if self.labels_of(project, showing, cx) == Some(labels.as_slice()) {
            return;
        }
        match showing {
            Showing::Session(id) => self.with_session(*id, cx, |chat| {
                chat.labels = labels;
                chat.flush();
            }),
            Showing::Article(id) => {
                let open = &mut self.projects[project];
                if let Some(ix) = open.article_ix(id) {
                    open.articles[ix].set_labels(labels);
                }
            }
            Showing::Board(id) => {
                self.with_board(id, cx, |board| board.labels = labels.clone());
            }
            Showing::Table(_) => return,
        }
        cx.notify();
    }

    /// An agent's labelling of the session filed under `record`.
    pub fn label_record(&mut self, record: &str, labels: Vec<String>, cx: &mut Context<Self>) {
        let Some(id) = self.session_by_record(record, cx).map(|chat| chat.id) else {
            return;
        };
        let Some(project) = self.project_of(id) else {
            return;
        };
        self.set_labels(project, &Showing::Session(id), labels, cx);
    }

    /// Every entry of every open project that can carry labels, by project
    /// and what it is.
    fn labelled(&self) -> Vec<(usize, Showing)> {
        let mut out = Vec::new();
        for (at, open) in self.projects.iter().enumerate() {
            out.extend(
                open.sessions
                    .iter()
                    .map(|chat| (at, Showing::Session(chat.id))),
            );
            out.extend(
                open.articles
                    .iter()
                    .map(|article| (at, Showing::Article(article.id.clone()))),
            );
            out.extend(
                open.boards
                    .iter()
                    .map(|board| (at, Showing::Board(board.id.clone()))),
            );
        }
        out
    }

    /// List only the entries of the project at `project` carrying `label`, or
    /// every one of them for `None`.
    pub fn set_project_label(
        &mut self,
        project: usize,
        label: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = self.projects.get_mut(project) {
            open.label = label;
            cx.notify();
        }
    }

    /// The labels the project at `project` carries, with how many of its
    /// entries carry each, in name order.
    pub fn project_label_counts(&self, project: usize, cx: &App) -> BTreeMap<String, usize> {
        self.counted(Some(project), cx)
    }

    /// Every label carried in any open project, with how many entries carry
    /// it, in name order.
    pub fn label_counts(&self, cx: &App) -> BTreeMap<String, usize> {
        self.counted(None, cx)
    }

    /// The labels carried in `project`, or in every open project for none.
    fn counted(&self, project: Option<usize>, cx: &App) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for (at, showing) in self.labelled() {
            if project.is_some_and(|project| project != at) {
                continue;
            }
            for label in self.labels_of(at, &showing, cx).unwrap_or_default() {
                *counts.entry(label.clone()).or_default() += 1;
            }
        }
        counts
    }

    /// Rename `from` to `to` on every entry carrying it. Onto a label already
    /// in use, the two become one.
    pub fn rename_label(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        let Some(to) = artifact::label::normalize(to) else {
            return;
        };
        self.relabel(from, Some(to), cx);
    }

    /// Take `name` off every entry carrying it.
    pub fn delete_label(&mut self, name: &str, cx: &mut Context<Self>) {
        self.relabel(name, None, cx);
    }

    fn relabel(&mut self, from: &str, to: Option<String>, cx: &mut Context<Self>) {
        for open in &mut self.projects {
            if open.label.as_deref() == Some(from) {
                open.label = to.clone();
            }
        }
        for (at, showing) in self.labelled() {
            let Some(held) = self.labels_of(at, &showing, cx) else {
                continue;
            };
            if held.iter().any(|label| label == from) {
                self.swap_label(at, &showing, from, to.clone(), cx);
            }
        }
    }

    /// Take `from` off one entry and put `to` on in its place.
    pub fn swap_label(
        &mut self,
        project: usize,
        showing: &Showing,
        from: &str,
        to: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(held) = self.labels_of(project, showing, cx) else {
            return;
        };
        let labels = held
            .iter()
            .filter(|label| *label != from)
            .cloned()
            .chain(to)
            .collect();
        self.set_labels(project, showing, labels, cx);
    }
}
