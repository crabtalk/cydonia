//! The project's tables: what a column may hold, and what a table is.
//! [`rows`] holds what is in one, [`Tables`] is what a backend answers,
//! `sqlite` is the backend on this machine and `remote` the client of one on
//! another.
//!
//! The shapes here name no connection: a table a client was told about and a
//! page of rows it was handed have the same shape whichever backend answered.
//!
//! A table's `key` is the SQL identifier and never moves; its `name` is free
//! text, so renaming one cannot break a query an agent already wrote.

#[cfg(feature = "remote")]
pub mod remote;
pub mod rows;
#[cfg(feature = "remote")]
pub mod service;
#[cfg(feature = "sqlite")]
pub mod sqlite;

use anyhow::Result;
use futures::future::BoxFuture;
use rows::{Edit, Page, Rows};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Where a store on this machine is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Home {
    /// A project's `.cydonia/`. Its tables are entries of the project,
    /// numbered with the project's other entries.
    Project(PathBuf),
    /// An app's own directory. Its tables are no project's and have no
    /// numbers.
    App(PathBuf),
}

/// What a [`Tables`] call answers with once the backend has.
pub type Reply<T> = BoxFuture<'static, Result<T>>;

/// A database of tables, read and written. A table is named by its key, its
/// display name, or, in a project's store, its `#N` reference.
///
/// A call is issued when it is made, not when its reply is awaited: calls on
/// one store land in the order they were made.
pub trait Tables {
    /// Every table with its columns and row count, last touched first.
    fn list(&self) -> Reply<Vec<Table>>;

    /// Put a table away, or bring it back. Its rows are untouched.
    fn archive(&mut self, key: &str, archived: bool) -> Reply<()>;

    /// Mark a table written just now — the stamp [`Tables::list`] orders on.
    fn touch(&mut self, key: &str) -> Reply<()>;

    /// Make a table. `key` overrides the identifier derived from `name`.
    fn create(
        &mut self,
        name: &str,
        key: Option<&str>,
        columns: &[Column],
        author: Option<&str>,
    ) -> Reply<Table>;

    /// Set a table's display name, its key, or both. Moving the key keeps its
    /// `#N` reference.
    fn update(&mut self, table: &str, name: Option<&str>, key: Option<&str>) -> Reply<Table>;

    /// Take a table away, its rows and its name together.
    fn remove(&mut self, table: &str) -> Reply<()>;

    /// Add the column `column` when the table has none of that name, which
    /// takes `kind`; otherwise rename it to `rename`, retype it to `kind`, or
    /// both.
    fn write_column(
        &mut self,
        table: &str,
        column: &str,
        kind: Option<ColType>,
        rename: Option<&str>,
    ) -> Reply<Vec<Column>>;

    /// A window of `limit` rows from `offset`, ordered by the column `sort`
    /// when the table has one of that name.
    fn read(
        &self,
        table: &str,
        sort: Option<&str>,
        desc: bool,
        limit: i64,
        offset: i64,
    ) -> Reply<Page>;

    /// Write cells, all of them or none.
    fn write_cells(&mut self, table: &str, edits: &[Edit]) -> Reply<()>;

    /// Append `count` empty rows, answering with their ids in order.
    fn add_rows(&mut self, table: &str, count: i64) -> Reply<Vec<i64>>;

    /// Append rows, each by column name, answering with their ids in order. A
    /// column the table does not have fails the lot.
    fn insert(&mut self, table: &str, rows: &[Map<String, Value>]) -> Reply<Vec<i64>>;

    /// Take away the rows whose `column` holds `value`.
    fn delete_where(&mut self, table: &str, column: &str, value: &Value) -> Reply<()>;

    fn delete_rows(&mut self, table: &str, rowids: &[i64]) -> Reply<()>;

    /// Take a column away. A table keeps at least one.
    fn drop_column(&mut self, table: &str, column: &str) -> Reply<Vec<Column>>;

    /// Run one `SELECT`. It cannot write.
    fn query(&self, sql: &str) -> Reply<Rows>;

    /// Run inserts, updates and deletes, all of them or none, answering with
    /// the rows touched.
    fn write(&mut self, sql: &str) -> Reply<u64>;
}

/// [`Tables`] kept on this machine, at a [`Home`].
pub trait Local: Tables + Sized {
    /// The store at `home`, made if there is none.
    fn open(home: &Home) -> Result<Self>;

    /// The store at `home` as it already is; `None` when there is none. Writes
    /// nothing to `home`.
    fn attach(home: &Home) -> Option<Self>;
}

/// What a column holds. Four, closed, and every one a word SQLite keeps
/// verbatim in its catalog — which is what lets a declaration be the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColType {
    Text,
    Number,
    Date,
    Check,
}

impl ColType {
    /// Every type, for the prompt that has to name the menu. Interpolated
    /// rather than retyped in prose, so adding one here reaches the model.
    pub const ALL: [Self; 4] = [Self::Text, Self::Number, Self::Date, Self::Check];

    /// The wire name — what the tools take and hand back.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::Check => "check",
        }
    }

    /// The declared type written into `CREATE TABLE`. `DATE` and `BOOLEAN` both
    /// take NUMERIC affinity, which is exactly right: a day is unix seconds and
    /// a checkbox is 0 or 1.
    pub fn sql(self) -> &'static str {
        match self {
            Self::Text => "TEXT",
            Self::Number => "NUMERIC",
            Self::Date => "DATE",
            Self::Check => "BOOLEAN",
        }
    }
}

/// Reading the catalog back. Total rather than fallible, because the catalog is
/// not our enum: a table made outside these tools may declare anything, and a
/// column we cannot name is still a column the person can read.
impl From<&str> for ColType {
    fn from(declared: &str) -> Self {
        match declared.to_ascii_uppercase().as_str() {
            "NUMERIC" => Self::Number,
            "DATE" => Self::Date,
            "BOOLEAN" => Self::Check,
            _ => Self::Text,
        }
    }
}

/// A column's name is its SQL identifier — quoted everywhere, so it stays free
/// text the way a spreadsheet header is. Only tables carry a key, because only
/// a table is addressed in prose after it is renamed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ColType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub number: Option<u64>,
    /// What `FROM` takes.
    pub key: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub rows: i64,
    pub created_at: i64,
    /// When it was last written in, once it has been — what the list is
    /// ordered on, with [`Table::created_at`] standing in until then.
    pub updated_at: Option<i64>,
    /// Put away: listed under the divider rather than dropped.
    pub archived: bool,
    /// The agent that made it, by the name `settings.toml` gives it.
    pub author: Option<String>,
}

impl Table {
    /// [`Table::created_at`] in the milliseconds every other stamp is kept in.
    pub fn created_ms(&self) -> u128 {
        seconds_to_ms(self.created_at)
    }

    /// When it was last written in, or made, in milliseconds.
    pub fn touched_ms(&self) -> u128 {
        seconds_to_ms(self.updated_at.unwrap_or(self.created_at))
    }
}

fn seconds_to_ms(seconds: i64) -> u128 {
    seconds.max(0) as u128 * 1000
}

/// `ident` as an SQL identifier: in double quotes, its own doubled.
pub fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}
