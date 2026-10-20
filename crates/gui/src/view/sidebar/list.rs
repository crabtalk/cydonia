//! What the sidebar lists, and in what order: read off the workspace and the
//! applied search. The sidebar draws it, and the window steps through it and
//! lands on it.

use super::{Group, Heading, Place, Row, shown};
use crate::{
    model::{state, workspace::Workspace},
    view::search::Search,
};
use artifact::kind::Kind;
use bezel::gpui::App;
use std::collections::HashMap;

/// One entry of a project, with what the list can be ordered by.
pub(super) struct Ranked {
    pub(super) archived: bool,
    touched: u128,
    /// Case-folded, for the comparison alone — the row draws its own name.
    name: String,
    pub(super) row: Row,
}

/// The sidebar's list as the workspace and the applied search have it.
pub(crate) struct List<'a> {
    workspace: &'a Workspace,
    search: &'a Search,
    cx: &'a App,
}

impl<'a> List<'a> {
    pub(crate) fn new(workspace: &'a Workspace, search: &'a Search, cx: &'a App) -> Self {
        Self {
            workspace,
            search,
            cx,
        }
    }

    /// Entries ordered by last user submission for sessions, last edit otherwise.
    ///
    /// One list rather than four: the kinds are told apart by their marks, and
    /// grouping by kind buries the table you are working in under every article
    /// you are not.
    pub(crate) fn entries(&self, project: usize) -> Vec<Row> {
        let features = &self.workspace.settings.features;
        let mut entries = self.ranked(project);
        // A project is read off disk whole whatever is switched on, so what a
        // switch hides it hides here — the entries stay in the project and in
        // memory, and turning it back on lists them again with nothing to
        // rescan.
        entries.retain(|entry| shown(&entry.row, features) && !self.filtered_out(&entry.row));
        let split = entries.iter().position(|entry| entry.archived);
        let mut entries = entries.into_iter().map(|entry| entry.row);
        let mut rows: Vec<Row> = entries.by_ref().take(split.unwrap_or(usize::MAX)).collect();
        if split.is_some()
            && let Some(open) = self.workspace.projects.get(project)
        {
            rows.push(Row::Archive(open.path.clone()));
            if open.archive_open {
                rows.extend(entries);
            }
        }
        rows
    }

    /// Every one of a project's entries in the order the sidebar lists them,
    /// before anything is hidden or folded away.
    ///
    /// What a drag rewrites, which is why it is this list and not the one on
    /// screen: a kind switched off is still in the project, and keeps the
    /// place it was put.
    pub(super) fn ranked(&self, project: usize) -> Vec<Ranked> {
        let (workspace, cx) = (self.workspace, self.cx);
        let Some(open) = workspace.projects.get(project) else {
            return Vec::new();
        };
        // Folded for the comparison and kept that way: a sort reads it many
        // times and the case is never shown from here.
        let folded = |name: &str| name.to_lowercase();
        let row = |showing| Row::Entry {
            project: open.path.clone(),
            showing,
        };
        // Apps are listed under their own heading — see [`Self::apps`].
        let mut entries: Vec<Ranked> = Kind::ALL
            .into_iter()
            .filter(|kind| *kind != Kind::App)
            .flat_map(|kind| open.every(kind, cx))
            // An article's thread is reached from its article.
            .filter(|facts| match facts.showing {
                crate::model::workspace::Showing::Session(id) => open
                    .session(id, cx)
                    .is_none_or(|chat| chat.article.is_none()),
                _ => true,
            })
            .map(|facts| Ranked {
                archived: facts.archived,
                touched: facts.touched,
                name: folded(&facts.label),
                row: row(facts.showing),
            })
            .collect();
        let sort = workspace.sort_of(project);
        // Archived entries sink and pinned ones rise, whatever the list is
        // ordered by: what is put away is out of the way, and a pin is a place
        // somebody asked for. The sort is what happens between them.
        let head = |entry: &Ranked| {
            let pin = entry
                .row
                .showing()
                .and_then(|showing| workspace.pin_rank(project, showing.clone(), cx));
            (entry.archived, pin.is_none(), pin.unwrap_or_default())
        };
        entries.sort_by(|a, b| {
            head(a).cmp(&head(b)).then_with(|| match sort {
                state::Sort::Name => a.name.cmp(&b.name),
                state::Sort::Touched => b.touched.cmp(&a.touched),
                // The arrangement, or the entry's stamp where there is none to
                // follow — an entry made since the order was written has no
                // rank yet, and is listed above the rows that do rather than
                // under them. So a new session arrives at the top of the
                // unpinned rows without displacing a pin.
                state::Sort::Manual => {
                    let rank = |entry: &Ranked| {
                        let at = entry
                            .row
                            .showing()
                            .and_then(|showing| workspace.rank_of(project, showing.clone(), cx));
                        (at.is_some(), at.unwrap_or_default())
                    };
                    rank(a)
                        .cmp(&rank(b))
                        .then_with(|| b.touched.cmp(&a.touched))
                }
            })
        });
        entries
    }

    /// The apps listed, by name, for the Apps heading.
    fn apps(&self) -> Vec<Row> {
        let mut apps: Vec<(String, Row)> = self
            .workspace
            .listed_apps(self.cx)
            .map(|held| {
                (
                    held.program.read(self.cx).label().to_lowercase(),
                    Row::App(held.id.clone()),
                )
            })
            .collect();
        apps.sort_by(|a, b| a.0.cmp(&b.0));
        apps.into_iter().map(|(_, row)| row).collect()
    }

    /// Whether `row` is an entry its project's label filter leaves out.
    fn filtered_out(&self, row: &Row) -> bool {
        let Row::Entry { project, showing } = row else {
            return false;
        };
        let workspace = self.workspace;
        let Some(at) = workspace.project_at(project) else {
            return false;
        };
        let Some(label) = &workspace.projects[at].label else {
            return false;
        };
        !workspace
            .labels_of(at, showing, self.cx)
            .is_some_and(|labels| labels.contains(label))
    }

    /// Whether `row` is an entry of a project listed by a label. Such a list
    /// leaves rows out, so its rows are not dragged.
    pub(super) fn narrowed(&self, row: &Row) -> bool {
        let Row::Entry { project, .. } = row else {
            return false;
        };
        let workspace = self.workspace;
        workspace
            .project_at(project)
            .is_some_and(|at| workspace.projects[at].label.is_some())
    }

    /// Whether a group's rows are folded away under it.
    pub(crate) fn folded(&self, group: &Group) -> bool {
        let workspace = self.workspace;
        let Group::Project(path) = group;
        workspace
            .project_at(path)
            .is_some_and(|ix| !workspace.projects[ix].expanded)
    }

    /// How far in a row is drawn: one step for a project's rows when that is
    /// switched on.
    ///
    /// The indent is the whole of what says a row belongs to the heading above
    /// it. Nothing else is drawn on it — a rule beside it or a wash behind it
    /// is a second way of saying what the offset already says.
    pub(super) fn indent_of(&self, row: &Row) -> u8 {
        match row {
            Row::Heading(_) | Row::App(_) => 0,
            _ => u8::from(self.workspace.indent_project_rows),
        }
    }

    /// Every line the sidebar shows, in order. Addresses only: a project with a
    /// thousand articles costs a thousand `Row`s here and reads a title for
    /// none of them.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let workspace = self.workspace;
        let projects = workspace
            .projects
            .iter()
            .map(|open| Group::Project(open.path.clone()));
        // The projects' heading whether or not any are open: it holds the way
        // to open one.
        let sections = vec![(Heading::Projects, projects.collect::<Vec<_>>())];
        let apps = self.apps();
        let mut rows = Vec::new();
        if let Some(matches) = self.search.rows(self.cx) {
            for (heading, groups) in sections {
                let start = rows.len();
                rows.push(Row::Heading(heading));
                for group in groups {
                    let Group::Project(path) = &group;
                    let members: Vec<_> = workspace
                        .project_at(path)
                        .map(|ix| self.ranked(ix))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|e| e.row)
                        .filter(|row| matches.contains(row) && !self.filtered_out(row))
                        .collect();
                    if !members.is_empty() {
                        rows.push(Row::Group(group));
                        rows.extend(members);
                    }
                }
                if rows.len() == start + 1 {
                    rows.pop();
                }
            }
            let apps: Vec<Row> = apps
                .into_iter()
                .filter(|row| matches.contains(row))
                .collect();
            if !apps.is_empty() {
                rows.push(Row::Heading(Heading::Apps));
                rows.extend(apps);
            }
            return rows;
        }
        for (heading, groups) in sections {
            rows.push(Row::Heading(heading));
            if workspace.section_folded(heading.key()) {
                continue;
            }
            for group in groups {
                let Group::Project(path) = &group;
                let open = !self.folded(&group);
                let members = open.then(|| {
                    workspace
                        .project_at(path)
                        .map(|ix| self.entries(ix))
                        .unwrap_or_default()
                });
                rows.push(Row::Group(group));
                rows.extend(members.into_iter().flatten());
            }
        }
        if !apps.is_empty() {
            rows.push(Row::Heading(Heading::Apps));
            if !workspace.section_folded(Heading::Apps.key()) {
                rows.extend(apps);
            }
        }
        rows
    }

    /// Every open project's entries still in use, most recently touched first,
    /// with what a switch hides left out.
    pub(crate) fn recent_rows(&self) -> Vec<Row> {
        let features = &self.workspace.settings.features;
        let mut entries: Vec<Ranked> = (0..self.workspace.projects.len())
            .flat_map(|project| self.ranked(project))
            .filter(|entry| !entry.archived && shown(&entry.row, features))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.touched));
        entries.into_iter().map(|entry| entry.row).collect()
    }

    /// Where each row sits. Entries stay within the region of their project
    /// they are listed in — the pins, the rest, the archive.
    pub(super) fn row_places(&self, rows: &[Row]) -> HashMap<Row, Place> {
        let mut section = Heading::Projects;
        rows.iter()
            .map(|row| {
                let kind = match row {
                    Row::Heading(heading) => {
                        section = *heading;
                        "heading".into()
                    }
                    Row::Archive(_) => "archive".into(),
                    Row::Group(Group::Project(_)) => "projects".into(),
                    Row::App(_) => "apps".into(),
                    Row::Entry { project, .. } => format!(
                        "entries-{}-{}-{}",
                        project.display(),
                        row.pinned(self.workspace, self.cx),
                        row.archived(self.workspace, self.cx),
                    )
                    .into(),
                };
                (row.clone(), Place { kind, section })
            })
            .collect()
    }
}
