use crate::data::{ColType, Data, Edit, Page, Table as Record};
use bezel::gpui::Context;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

const PAGE: i64 = 200;
const COLUMN: &str = "Column";

pub struct Table {
    pub record: Record,
    pub page: Option<Page>,
    pub error: Option<String>,
    data: Rc<RefCell<Data>>,
}

impl Table {
    pub fn new(record: Record, data: Rc<RefCell<Data>>) -> Self {
        Self {
            record,
            page: None,
            error: None,
            data,
        }
    }

    pub fn load(&mut self, cx: &mut Context<Self>) {
        if self.page.is_none() && self.read_page() {
            cx.notify();
        }
    }

    pub fn adopt(&mut self, record: Record, cx: &mut Context<Self>) {
        let mut changed = self.record != record;
        self.record = record;
        if self.page.is_some() {
            changed |= self.read_page();
        }
        if changed {
            cx.notify();
        }
    }

    fn read_page(&mut self) -> bool {
        match self
            .data
            .borrow()
            .read(&self.record.key, None, false, PAGE, 0)
        {
            Ok(page) => {
                let changed = self.page.as_ref() != Some(&page);
                self.page = Some(page);
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
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        };
        self.edit(cx, |data, key| {
            data.write_cells(
                key,
                &[Edit {
                    rowid,
                    column,
                    value,
                }],
            )
        })
    }

    pub fn add_row(&mut self, cx: &mut Context<Self>) -> Result<i64, String> {
        self.edit(cx, |data, key| Ok(data.add_rows(key, 1)?[0]))
    }

    pub fn delete_row(&mut self, rowid: i64, cx: &mut Context<Self>) -> Result<(), String> {
        self.edit(cx, |data, key| data.delete_rows(key, &[rowid]))
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
                if self.page.is_some() {
                    self.read_page();
                }
            }
            Err(error) => self.error = Some(error.clone()),
        }
        cx.notify();
        result
    }
}
