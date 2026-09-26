//! No database. [`Data`] has no values: [`Data::attach`] finds none, so a
//! project's `Option<Data>` is always empty and nothing here can be called.

use super::{ColType, Column, Edit, Page, Rows, Table};
use anyhow::{Result, bail};
use std::path::Path;

pub enum Data {}

impl Data {
    pub fn open(project: &Path) -> Result<Self> {
        bail!("{} has no database in this build", project.display())
    }

    pub fn attach(project: &Path) -> Option<Self> {
        let _ = project;
        None
    }

    pub fn list(&self) -> Result<Vec<Table>> {
        match *self {}
    }

    pub fn archive(&mut self, _: &str, _: bool) -> Result<()> {
        match *self {}
    }

    pub fn touch(&mut self, _: &str) -> Result<()> {
        match *self {}
    }

    pub fn create(
        &mut self,
        _: &str,
        _: Option<&str>,
        _: &[Column],
        _: Option<&str>,
    ) -> Result<Table> {
        match *self {}
    }

    pub fn update(&mut self, _: &str, _: Option<&str>, _: Option<&str>) -> Result<Table> {
        match *self {}
    }

    pub fn remove(&mut self, _: &str) -> Result<()> {
        match *self {}
    }

    pub fn write_column(
        &mut self,
        _: &str,
        _: &str,
        _: Option<ColType>,
        _: Option<&str>,
    ) -> Result<Vec<Column>> {
        match *self {}
    }

    pub fn read(&self, _: &str, _: Option<&str>, _: bool, _: i64, _: i64) -> Result<Page> {
        match *self {}
    }

    pub fn write_cells(&mut self, _: &str, _: &[Edit]) -> Result<()> {
        match *self {}
    }

    pub fn add_rows(&mut self, _: &str, _: i64) -> Result<Vec<i64>> {
        match *self {}
    }

    pub fn delete_rows(&mut self, _: &str, _: &[i64]) -> Result<()> {
        match *self {}
    }

    pub fn drop_column(&mut self, _: &str, _: &str) -> Result<Vec<Column>> {
        match *self {}
    }

    pub fn query(&self, _: &str) -> Result<Rows> {
        match *self {}
    }

    pub fn write(&mut self, _: &str) -> Result<u64> {
        match *self {}
    }
}
