//! No database. [`Absent`] has no values: [`Absent::attach`] finds none and
//! [`Absent::open`] fails, so nothing here can be called.

use super::{ColType, Column, Edit, Home, Local, Page, Reply, Rows, Table, Tables};
use anyhow::{Result, bail};

pub enum Absent {}

impl Local for Absent {
    fn open(_: &Home) -> Result<Self> {
        bail!("There are no tables in this build")
    }

    fn attach(_: &Home) -> Option<Self> {
        None
    }
}

impl Tables for Absent {
    fn list(&self) -> Reply<Vec<Table>> {
        match *self {}
    }

    fn archive(&mut self, _: &str, _: bool) -> Reply<()> {
        match *self {}
    }

    fn touch(&mut self, _: &str) -> Reply<()> {
        match *self {}
    }

    fn create(&mut self, _: &str, _: Option<&str>, _: &[Column], _: Option<&str>) -> Reply<Table> {
        match *self {}
    }

    fn update(&mut self, _: &str, _: Option<&str>, _: Option<&str>) -> Reply<Table> {
        match *self {}
    }

    fn remove(&mut self, _: &str) -> Reply<()> {
        match *self {}
    }

    fn write_column(
        &mut self,
        _: &str,
        _: &str,
        _: Option<ColType>,
        _: Option<&str>,
    ) -> Reply<Vec<Column>> {
        match *self {}
    }

    fn read(&self, _: &str, _: Option<&str>, _: bool, _: i64, _: i64) -> Reply<Page> {
        match *self {}
    }

    fn write_cells(&mut self, _: &str, _: &[Edit]) -> Reply<()> {
        match *self {}
    }

    fn add_rows(&mut self, _: &str, _: i64) -> Reply<Vec<i64>> {
        match *self {}
    }

    fn insert(
        &mut self,
        _: &str,
        _: &[serde_json::Map<String, serde_json::Value>],
    ) -> Reply<Vec<i64>> {
        match *self {}
    }

    fn delete_where(&mut self, _: &str, _: &str, _: &serde_json::Value) -> Reply<()> {
        match *self {}
    }

    fn delete_rows(&mut self, _: &str, _: &[i64]) -> Reply<()> {
        match *self {}
    }

    fn drop_column(&mut self, _: &str, _: &str) -> Reply<Vec<Column>> {
        match *self {}
    }

    fn query(&self, _: &str) -> Reply<Rows> {
        match *self {}
    }

    fn write(&mut self, _: &str) -> Reply<u64> {
        match *self {}
    }
}
