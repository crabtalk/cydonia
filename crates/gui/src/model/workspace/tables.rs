use crate::{
    data::{ColType, Column, Data, Home},
    model::{
        table::Table,
        workspace::{Entry, Workspace},
    },
};
use artifact::kind::Kind;
use bezel::gpui::{App, Context, Entity};
use std::{cell::RefCell, rc::Rc};

const UNTITLED: &str = "Untitled";

impl Workspace {
    pub fn new_table(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let ix = self.create_table(cx)?;
        self.open_table(self.active?, ix, cx);
        Some(ix)
    }

    pub fn create_table(&mut self, cx: &mut App) -> Option<usize> {
        if !self.settings.features.tables {
            return None;
        }
        let project = self.projects.get_mut(self.active?)?;
        if project.data.is_none() {
            project.data = Data::open(&Home::Project(project.path.clone()))
                .ok()
                .map(|data| Rc::new(RefCell::new(data)));
        }
        let mut name = UNTITLED.to_owned();
        for n in 2.. {
            if !project
                .tables
                .iter()
                .any(|held| held.table.read(cx).record.name == name)
            {
                break;
            }
            name = format!("{UNTITLED} {n}");
        }
        let key = project
            .data
            .as_ref()?
            .borrow_mut()
            .create(
                &name,
                None,
                &[Column {
                    name: "Name".into(),
                    kind: ColType::Text,
                }],
                None,
            )
            .ok()?
            .key;
        project.reload_tables(cx);
        project.table_ix(&key)
    }

    pub fn open_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let Some(held) = open.tables.get(ix) else {
            return;
        };
        let id = held.key.clone();
        open.table = Some(ix);
        open.load_table(ix, cx);
        self.active = Some(project);
        self.remember(project, Kind::Table, id, cx);
        cx.notify();
    }

    pub fn delete_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let Some(held) = open.tables.get(ix) else {
            return;
        };
        let Some(data) = open.data.as_ref() else {
            return;
        };
        if let Err(error) = data.borrow_mut().remove(&held.key) {
            held.table.update(cx, |table, cx| {
                table.error = Some(error.to_string());
                cx.notify();
            });
            return;
        }
        open.reload_tables(cx);
        cx.notify();
    }

    pub fn table_entity(&self, entry: &Entry) -> Option<&Entity<Table>> {
        if !self.settings.features.tables || entry.kind != Kind::Table {
            return None;
        }
        let project = self.project(entry.project)?;
        Some(&project.tables.get(project.table_ix(&entry.id)?)?.table)
    }

    pub fn edit_table<T>(
        &mut self,
        entry: &Entry,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Table, &mut Context<Table>) -> Result<T, String>,
    ) -> Result<T, String> {
        let source = self
            .table_entity(entry)
            .cloned()
            .ok_or("table is no longer open")?;
        let result = source.update(cx, edit);
        if result.is_ok() {
            if self.preview.as_ref().and_then(|member| member.entry()) == Some(entry) {
                self.preview = None;
            }
            let project = self
                .projects
                .iter_mut()
                .find(|project| project.id == entry.project)
                .expect("table belongs to an open project");
            project.reload_tables(cx);
            cx.notify();
        }
        result
    }

    pub fn rename_table(
        &mut self,
        entry: &Entry,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit_table(entry, cx, |table, cx| table.rename(&name, cx))
    }

    pub fn archive_table(
        &mut self,
        entry: &Entry,
        archived: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit_table(entry, cx, |table, cx| table.archive(archived, cx))
    }
}
