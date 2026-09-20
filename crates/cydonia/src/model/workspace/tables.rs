//! The tables a project holds: the store behind them, and every edit that
//! reaches one.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
    /// A fresh table in the active project, opened as it lands.
    ///
    /// One text column, because the store will not make a table without one
    /// and a column you can rename is a better start than a dialog asking for
    /// the shape before anything exists to shape.
    pub fn new_table(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        if !self.settings.features.tables {
            return None;
        }
        let at = self.active?;
        let project = self.projects.get_mut(at)?;
        // The one place a store is created: making a table is the moment the
        // project has something to keep in one.
        if project.data.is_none() {
            project.data = Data::open(&project.path).ok();
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
        let key = project
            .data
            .as_mut()?
            .create(&name, None, &[column], None)
            .ok()?
            .key;
        project.reload_tables();
        // Found by key rather than taken as a known row: the list is ordered by
        // age, and where the newest lands is the list's business, not this one's.
        let ix = project.tables.iter().position(|table| table.key == key)?;
        self.reveal_project(at, cx);
        self.open_table(at, ix, cx);
        Some(ix)
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
        open.reload_page();
        let id = open.tables[ix].key.clone();
        self.active = Some(project);
        self.remember(project, state::Kind::Table, id, cx);
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
        if let Some(data) = open.data.as_mut() {
            let _ = data.remove(&key);
        }
        open.table = open
            .table
            .filter(|shown| *shown != ix)
            .map(|shown| if shown > ix { shown - 1 } else { shown });
        open.reload_tables();
        cx.notify();
    }

    /// Run `f` against the open table's store, then re-read what it did.
    ///
    /// Every table mutation goes through here, so none of them can forget the
    /// reload — a grid still showing the row you just deleted is the bug this
    /// shape makes unwritable.
    fn with_table<T>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Data, &str) -> T,
    ) -> Option<T> {
        let at = self.active?;
        let project = self.projects.get_mut(at)?;
        let key = project
            .table
            .and_then(|ix| project.tables.get(ix))
            .map(|table| table.key.clone())?;
        let data = project.data.as_mut()?;
        let done = f(data, &key);
        // Working in a table is what makes it the table you were last in, and
        // the list is ordered by that.
        let _ = data.touch(&key);
        project.reload_tables();
        cx.notify();
        Some(done)
    }

    /// The name of column `at`, which is what the store addresses one by.
    fn column_name(&self, at: usize) -> Option<String> {
        let page = self.active_page()?;
        page.columns.get(at).map(|column| column.name.clone())
    }

    /// Write one cell. The text goes in as text whatever the column holds —
    /// SQLite's affinity converts it on the way, so a number typed into a
    /// number column lands as one and the same text in a text column stays put.
    pub fn write_cell(&mut self, rowid: i64, at: usize, text: String, cx: &mut Context<Self>) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        let value = match text.is_empty() {
            true => serde_json::Value::Null,
            false => serde_json::Value::String(text),
        };
        self.with_table(cx, |data, key| {
            let _ = data.write_cells(
                key,
                &[Edit {
                    rowid,
                    column,
                    value,
                }],
            );
        });
    }

    pub fn add_row(&mut self, cx: &mut Context<Self>) -> Option<i64> {
        self.with_table(cx, |data, key| {
            data.add_rows(key, 1)
                .ok()
                .and_then(|ids| ids.first().copied())
        })
        .flatten()
    }

    pub fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) {
        self.with_table(cx, |data, key| {
            let _ = data.delete_rows(key, &[rowid]);
        });
    }

    /// A fresh text column, named so it does not collide with one already
    /// there — the header is where it gets its real name.
    pub fn add_column(&mut self, cx: &mut Context<Self>) {
        let taken: Vec<String> = self
            .active_page()
            .map(|page| page.columns.iter().map(|col| col.name.clone()).collect())
            .unwrap_or_default();
        let mut name = COLUMN.to_owned();
        for n in 2.. {
            if !taken.contains(&name) {
                break;
            }
            name = format!("{COLUMN} {n}");
        }
        self.with_table(cx, |data, key| {
            let _ = data.write_column(key, &name, Some(ColType::Text), None);
        });
    }

    /// Rename column `at`, retype it, or both — one call, as the store has it.
    pub fn write_column(
        &mut self,
        at: usize,
        kind: Option<ColType>,
        rename: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        self.with_table(cx, |data, key| {
            let _ = data.write_column(key, &column, kind, rename.as_deref());
        });
    }

    pub fn delete_column(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(column) = self.column_name(at) else {
            return;
        };
        self.with_table(cx, |data, key| {
            let _ = data.drop_column(key, &column);
        });
    }

    /// The display name only. The key stays where it is, so a query already
    /// written against this table goes on running.
    pub fn rename_table(&mut self, key: &str, name: String, cx: &mut Context<Self>) {
        self.with_store(key, cx, |data, key| {
            let _ = data.update(key, Some(name.trim()), None);
        });
    }

    pub fn archive_table(&mut self, key: &str, archived: bool, cx: &mut Context<Self>) {
        self.with_store(key, cx, |data, key| {
            let _ = data.archive(key, archived);
        });
        self.prune_archived(cx);
    }

    /// Run `f` against whichever store holds `key`, then re-read what it did.
    /// Named rather than open: the sidebar acts on rows the pane is not showing.
    fn with_store(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Data, &str),
    ) -> Option<()> {
        let project = self
            .projects
            .iter_mut()
            .find(|open| open.tables.iter().any(|table| table.key == key))?;
        f(project.data.as_mut()?, key);
        project.reload_tables();
        cx.notify();
        Some(())
    }

    /// The rows on screen. Gated beside [`Self::active_table`]: the table pane
    /// reads the page, not the table, so both have to be shut for it to close.
    pub fn active_page(&self) -> Option<&Page> {
        if !self.settings.features.tables {
            return None;
        }
        self.active_project()?.open_page()
    }

    pub fn active_table(&self) -> Option<&Table> {
        if !self.settings.features.tables {
            return None;
        }
        let project = self.active_project()?;
        project.tables.get(project.table?)
    }
}
