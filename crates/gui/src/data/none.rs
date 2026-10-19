//! No database. [`Absent`] has no values: [`Absent::attach`] finds none and
//! [`Absent::open`] fails, so nothing here can be called.

use super::{ColType, Column, Edit, Home, Local, Page, Rows, Table, Tables};
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
    fn list(&self) -> Result<Vec<Table>> {
        match *self {}
    }

    fn archive(&mut self, _: &str, _: bool) -> Result<()> {
        match *self {}
    }

    fn touch(&mut self, _: &str) -> Result<()> {
        match *self {}
    }

    fn create(&mut self, _: &str, _: Option<&str>, _: &[Column], _: Option<&str>) -> Result<Table> {
        match *self {}
    }

    fn update(&mut self, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Table> {
        match *self {}
    }

    fn remove(&mut self, _: &str) -> Result<()> {
        match *self {}
    }

    fn write_column(
        &mut self,
        _: &str,
        _: &str,
        _: Option<ColType>,
        _: Option<&str>,
    ) -> Result<Vec<Column>> {
        match *self {}
    }

    fn read(&self, _: &str, _: Option<&str>, _: bool, _: i64, _: i64) -> Result<Page> {
        match *self {}
    }

    fn write_cells(&mut self, _: &str, _: &[Edit]) -> Result<()> {
        match *self {}
    }

    fn add_rows(&mut self, _: &str, _: i64) -> Result<Vec<i64>> {
        match *self {}
    }

    fn insert(
        &mut self,
        _: &str,
        _: &[serde_json::Map<String, serde_json::Value>],
    ) -> Result<Vec<i64>> {
        match *self {}
    }

    fn delete_where(&mut self, _: &str, _: &str, _: &serde_json::Value) -> Result<()> {
        match *self {}
    }

    fn delete_rows(&mut self, _: &str, _: &[i64]) -> Result<()> {
        match *self {}
    }

    fn drop_column(&mut self, _: &str, _: &str) -> Result<Vec<Column>> {
        match *self {}
    }

    fn query(&self, _: &str) -> Result<Rows> {
        match *self {}
    }

    fn write(&mut self, _: &str) -> Result<u64> {
        match *self {}
    }
}
