//! The tables a project holds: the store behind them, and every edit that
//! reaches one.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use crate::data::Reply;
use artifact::kind::Kind;

impl Workspace {
    /// A fresh table in the active project, opened as it lands. Answers its
    /// project and its place in that project's tables.
    ///
    /// One text column, because the store will not make a table without one
    /// and a column you can rename is a better start than a dialog asking for
    /// the shape before anything exists to shape.
    pub fn new_table(&mut self, cx: &mut Context<Self>) -> Task<Option<(usize, usize)>> {
        let made = self.create_table(cx);
        cx.spawn(async move |workspace, cx| {
            let (project, ix) = made.await?;
            workspace
                .update(cx, |workspace, cx| workspace.open_table(project, ix, cx))
                .ok()?;
            Some((project, ix))
        })
    }

    /// The same, not opened — what a space's pane takes as a tab without
    /// leaving the space. Answers once the store has made it and the list
    /// naming it has landed.
    pub fn create_table(&mut self, cx: &mut Context<Self>) -> Task<Option<(usize, usize)>> {
        if !self.settings.features.tables {
            return Task::ready(None);
        }
        let Some(project) = self.active.and_then(|at| self.projects.get_mut(at)) else {
            return Task::ready(None);
        };
        // The one place a store is created: making a table is the moment the
        // project has something to keep in one.
        if project.data.is_none() {
            project.data = project.store().make_tables().ok();
        }
        let mut name = UNTITLED.to_owned();
        for n in 2.. {
            if !project.tables.iter().any(|table| table.name == name) {
                break;
            }
            name = format!("{UNTITLED} {n}");
        }
        let column = Column {
            name: "Name".to_owned(),
            kind: ColType::Text,
        };
        let Some(data) = project.data.as_mut() else {
            return Task::ready(None);
        };
        let made = data.create(&name, None, &[column], None);
        let listed = project.reload_tables(cx);
        let location = project.location().clone();
        cx.spawn(async move |workspace, cx| {
            let key = made.await.ok()?.key;
            listed.await;
            workspace
                .update(cx, |workspace, _| {
                    let project = workspace.project_at(&location)?;
                    // Found by key rather than taken as a known row: the list
                    // is ordered by age, and where the newest lands is the
                    // list's business, not this one's.
                    Some((project, workspace.projects[project].table_ix(&key)?))
                })
                .ok()
                .flatten()
        })
    }

    /// Every project's tables are on show, so picking one brings its project
    /// forward with it.
    pub fn open_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= open.tables.len() {
            return;
        }
        open.table = Some(ix);
        open.reload_page(cx);
        let id = open.tables[ix].key.clone();
        self.active = Some(project);
        self.remember(project, Kind::Table, id, cx);
        cx.notify();
    }

    /// Drop the table: its rows go with it.
    pub fn delete_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let Some(key) = open.tables.get(ix).map(|table| table.key.clone()) else {
            return;
        };
        let Some(data) = open.data.as_mut() else {
            return;
        };
        let removed = data.remove(&key);
        // The open table is held by key across the re-read; only the one
        // dropped is let go of here.
        open.table = open.table.filter(|shown| *shown != ix);
        let listed = open.reload_tables(cx);
        cx.spawn(async move |_, _| {
            let _ = removed.await;
            listed.await;
        })
        .detach();
        cx.notify();
    }

    /// Read the tables of the project at `ix` for the first time. The entry it
    /// was last left on is found again once they land, when that is a table.
    pub(super) fn list_tables(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(ix) else {
            return;
        };
        let location = open.location().clone();
        let listed = open.reload_tables(cx);
        cx.spawn(async move |workspace, cx| {
            listed.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let active = workspace
                    .active_project()
                    .is_some_and(|open| *open.location() == location);
                let table = workspace
                    .last
                    .get(&location)
                    .is_some_and(|entry| entry.kind == Kind::Table);
                if active && table {
                    workspace.open_last_entry(cx);
                }
            });
        })
        .detach();
    }

    /// The name of column `at` of one table, which is what the store addresses
    /// one by.
    fn column_name(&self, key: &str, at: usize) -> Option<String> {
        let page = self.page_at(key)?;
        page.columns.get(at).map(|column| column.name.clone())
    }

    /// Write one cell. The text goes in as text whatever the column holds —
    /// SQLite's affinity converts it on the way, so a number typed into a
    /// number column lands as one and the same text in a text column stays put.
    pub fn write_cell(
        &mut self,
        key: &str,
        rowid: i64,
        at: usize,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self.column_name(key, at) else {
            return;
        };
        let value = match text.is_empty() {
            true => serde_json::Value::Null,
            false => serde_json::Value::String(text),
        };
        self.with_store(key, cx, |data, key| {
            data.write_cells(
                key,
                &[Edit {
                    rowid,
                    column,
                    value,
                }],
            )
        })
        .detach();
    }

    /// Append an empty row. Answers its id once it and the page holding it
    /// have landed.
    pub fn add_row(&mut self, key: &str, cx: &mut Context<Self>) -> Task<Option<i64>> {
        let added = self.with_store(key, cx, |data, key| data.add_rows(key, 1));
        cx.spawn(async move |_, _| added.await?.first().copied())
    }

    pub fn delete_row(&mut self, key: &str, rowid: i64, cx: &mut Context<Self>) {
        self.with_store(key, cx, |data, key| data.delete_rows(key, &[rowid]))
            .detach();
    }

    /// A fresh text column, named so it does not collide with one already
    /// there — the header is where it gets its real name.
    pub fn add_column(&mut self, key: &str, cx: &mut Context<Self>) {
        let taken: Vec<String> = self
            .page_at(key)
            .map(|page| page.columns.iter().map(|col| col.name.clone()).collect())
            .unwrap_or_default();
        let mut name = COLUMN.to_owned();
        for n in 2.. {
            if !taken.contains(&name) {
                break;
            }
            name = format!("{COLUMN} {n}");
        }
        self.with_store(key, cx, |data, key| {
            data.write_column(key, &name, Some(ColType::Text), None)
        })
        .detach();
    }

    /// Rename column `at`, retype it, or both — one call, as the store has it.
    pub fn write_column(
        &mut self,
        key: &str,
        at: usize,
        kind: Option<ColType>,
        rename: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self.column_name(key, at) else {
            return;
        };
        self.with_store(key, cx, |data, key| {
            data.write_column(key, &column, kind, rename.as_deref())
        })
        .detach();
    }

    pub fn delete_column(&mut self, key: &str, at: usize, cx: &mut Context<Self>) {
        let Some(column) = self.column_name(key, at) else {
            return;
        };
        self.with_store(key, cx, |data, key| data.drop_column(key, &column))
            .detach();
    }

    /// The display name only. The key stays where it is, so a query already
    /// written against this table goes on running.
    pub fn rename_table(&mut self, key: &str, name: String, cx: &mut Context<Self>) {
        self.with_store(key, cx, |data, key| {
            data.update(key, Some(name.trim()), None)
        })
        .detach();
    }

    pub fn archive_table(&mut self, key: &str, archived: bool, cx: &mut Context<Self>) {
        let archive = self.with_store(key, cx, |data, key| data.archive(key, archived));
        cx.spawn(async move |workspace, cx| {
            archive.await;
            let _ = workspace.update(cx, |workspace, cx| workspace.prune_archived(cx));
        })
        .detach();
    }

    /// Run `f` against whichever store holds `key`, then re-read what it did.
    /// Answers what `f`'s call answered once that and the re-read have landed;
    /// `None` when it failed.
    ///
    /// Every table mutation goes through here, named by the table: none of
    /// them can forget the reload — a grid still showing the row you just
    /// deleted is the bug this shape makes unwritable — and none of them can
    /// land in a table other than the one asked for, which the window can have
    /// two of on screen.
    fn with_store<T: 'static>(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Data, &str) -> Reply<T>,
    ) -> Task<Option<T>> {
        let Some(project) = self
            .projects
            .iter_mut()
            .find(|open| open.tables.iter().any(|table| table.key == key))
        else {
            return Task::ready(None);
        };
        let Some(data) = project.data.as_mut() else {
            return Task::ready(None);
        };
        let done = f(data, key);
        // Working in a table is what makes it the table you were last in, and
        // the list is ordered by that.
        let touched = data.touch(key);
        let reloaded = project.reload_tables(cx);
        cx.spawn(async move |_, _| {
            let done = done.await.ok();
            let _ = touched.await;
            reloaded.await;
            done
        })
    }

    /// Hold the pages of the tables drawn outside the table pane, by project
    /// path and key, letting go of every other held page.
    pub fn hold_pages(&mut self, held: &[(Location, String)], cx: &mut Context<Self>) {
        for open in &mut self.projects {
            let keys = held
                .iter()
                .filter(|(location, _)| location == open.location())
                .map(|(_, key)| key.clone())
                .collect();
            open.hold_pages(keys, cx);
        }
    }

    /// The rows of one table, wherever it is open.
    pub fn page_at(&self, key: &str) -> Option<&Page> {
        if !self.settings.features.tables {
            return None;
        }
        self.projects.iter().find_map(|open| open.pages.get(key))
    }

    pub fn active_table(&self) -> Option<&Table> {
        if !self.settings.features.tables {
            return None;
        }
        let project = self.active_project()?;
        project.tables.get(project.table?)
    }
}
