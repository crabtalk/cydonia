use crate::data::{ColType, Data, Edit, Page, Revision, Table as Record};
use bezel::gpui::Context;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub const PAGE: i64 = 200;
const COLUMN: &str = "Column";

#[derive(Clone, PartialEq, Eq)]
pub struct Sort {
    pub column: String,
    pub descending: bool,
}

pub struct Table {
    pub record: Record,
    pub sort: Option<Sort>,
    pages: BTreeMap<i64, Page>,
    pub error: Option<String>,
    data: Rc<RefCell<Data>>,
}

impl Table {
    pub fn new(record: Record, data: Rc<RefCell<Data>>) -> Self {
        Self {
            record,
            sort: None,
            pages: BTreeMap::new(),
            error: None,
            data,
        }
    }

    pub fn page(&self, offset: i64) -> Option<&Page> {
        self.pages.get(&offset)
    }

    pub fn last_page(&self) -> i64 {
        (self.record.rows.saturating_sub(1) / PAGE) * PAGE
    }

    pub fn load(&mut self, cx: &mut Context<Self>) {
        self.load_page(0, cx);
    }

    pub fn load_page(&mut self, offset: i64, cx: &mut Context<Self>) {
        if !self.pages.contains_key(&offset) && self.read_page(offset) {
            cx.notify();
        }
    }

    pub fn set_sort(&mut self, sort: Option<Sort>, cx: &mut Context<Self>) {
        if self.sort != sort {
            self.sort = sort;
            self.reload_pages();
            cx.notify();
        }
    }

    pub fn adopt(&mut self, record: Record, cx: &mut Context<Self>) {
        let mut changed = self.record != record;
        self.record = record;
        changed |= self.reload_pages();
        if changed {
            cx.notify();
        }
    }

    fn reload_pages(&mut self) -> bool {
        if let Some(sort) = &self.sort
            && !self
                .record
                .columns
                .iter()
                .any(|column| column.name == sort.column)
        {
            self.sort = None;
        }
        if self.pages.is_empty() {
            return false;
        }
        let last = self.last_page();
        let mut offsets: Vec<_> = self
            .pages
            .keys()
            .copied()
            .filter(|offset| *offset <= last)
            .collect();
        offsets.push(last);
        offsets.sort_unstable();
        offsets.dedup();
        self.pages.retain(|offset, _| *offset <= last);
        let mut changed = false;
        for offset in offsets {
            changed |= self.read_page(offset);
        }
        changed
    }

    fn read_page(&mut self, offset: i64) -> bool {
        let column = self.sort.as_ref().map(|sort| sort.column.as_str());
        let descending = self.sort.as_ref().is_some_and(|sort| sort.descending);
        match self
            .data
            .borrow()
            .read(&self.record.key, column, descending, PAGE, offset)
        {
            Ok(page) => {
                let changed = self.pages.get(&offset) != Some(&page);
                self.pages.insert(offset, page);
                changed
            }
            Err(error) => {
                let error = error.to_string();
                let changed = self.error.as_ref() != Some(&error);
                self.error = Some(error);
                changed
            }
        }
    }

    pub fn write_cell(
        &mut self,
        rowid: i64,
        column: String,
        text: String,
        revision: Option<Revision>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        };
        self.edit(cx, |data, key| {
            data.write_cells_at(
                key,
                &[Edit {
                    rowid,
                    column,
                    value,
                }],
                revision,
            )
        })
    }

    pub fn add_row(&mut self, cx: &mut Context<Self>) -> Result<i64, String> {
        self.edit(cx, |data, key| Ok(data.add_rows(key, 1)?[0]))
    }

    pub fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) -> Result<(), String> {
        self.edit(cx, |data, key| data.delete_rows(key, &[rowid]))
    }

    pub fn move_row(
        &mut self,
        row: i64,
        after: Option<i64>,
        before: Option<i64>,
        revision: Revision,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit(cx, |data, key| {
            data.move_row(key, row, after, before, revision)
        })
    }

    pub fn add_column(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let mut name = COLUMN.to_owned();
        for n in 2.. {
            if !self
                .record
                .columns
                .iter()
                .any(|column| column.name.eq_ignore_ascii_case(&name))
            {
                break;
            }
            name = format!("{COLUMN} {n}");
        }
        self.edit(cx, |data, key| {
            data.write_column(key, &name, Some(ColType::Text), None)
                .map(|_| ())
        })
    }

    pub fn write_column(
        &mut self,
        column: &str,
        kind: Option<ColType>,
        rename: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit(cx, |data, key| {
            data.write_column(key, column, kind, rename).map(|_| ())
        })
    }

    pub fn delete_column(&mut self, column: &str, cx: &mut Context<Self>) -> Result<(), String> {
        self.edit(cx, |data, key| data.drop_column(key, column).map(|_| ()))
    }

    pub fn write_option(
        &mut self,
        column: &str,
        previous: Option<&str>,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit(cx, |data, key| {
            data.write_option(key, column, previous, name)
        })
    }

    pub fn set_column_width(
        &mut self,
        column: &str,
        width: u32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit(cx, |data, key| data.set_column_width(key, column, width))
    }

    pub fn move_column(
        &mut self,
        column: &str,
        after: Option<&str>,
        before: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.edit(cx, |data, key| data.move_column(key, column, after, before))
    }

    pub fn rename(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        self.edit(cx, |data, key| {
            data.update(key, Some(name.trim()), None).map(|_| ())
        })
    }

    pub fn archive(&mut self, archived: bool, cx: &mut Context<Self>) -> Result<(), String> {
        self.edit(cx, |data, key| data.archive(key, archived))
    }

    fn edit<T>(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Data, &str) -> anyhow::Result<T>,
    ) -> Result<T, String> {
        let result =
            edit(&mut self.data.borrow_mut(), &self.record.key).map_err(|error| error.to_string());
        match &result {
            Ok(_) => {
                self.error = None;
                let mut data = self.data.borrow_mut();
                // The data write has committed even if refreshing its metadata fails.
                if let Err(error) = data.touch(&self.record.key) {
                    self.error = Some(error.to_string());
                }
                match data.table(&self.record.key) {
                    Ok(record) => self.record = record,
                    Err(error) => self.error = Some(error.to_string()),
                }
                drop(data);
                self.reload_pages();
            }
            Err(error) => self.error = Some(error.clone()),
        }
        cx.notify();
        result
    }
}
