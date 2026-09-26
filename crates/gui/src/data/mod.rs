//! The project's database: SQL tables the agent creates and queries, in a file
//! of its own under the project's `.cydonia/`.
//!
//! SQLite's catalog is the registry — columns are read back out of `PRAGMA
//! table_info`, so there is no second list of them to fall out of step with the
//! first, which is the drift that rots this kind of feature. A table's `key` is
//! the SQL identifier and never moves; its `name` is free text, so renaming one
//! cannot break a query the agent already wrote. Reads go to a handle opened
//! read-only: what stops a `SELECT` writing is the connection itself, never
//! anything believed about the text.

// The shapes a store answers with live in `artifact::table` — they are what
// the app draws and what a client reads.
pub use artifact::table::{
    ColType, Column, Table,
    rows::{Edit, Page, Row, Rows},
};

pub(crate) const FILE: &str = artifact::project::fs::DATA;

#[cfg(feature = "desktop")]
mod sqlite;
#[cfg(feature = "desktop")]
pub use sqlite::Data;

#[cfg(not(feature = "desktop"))]
mod none;
#[cfg(not(feature = "desktop"))]
pub use none::Data;
