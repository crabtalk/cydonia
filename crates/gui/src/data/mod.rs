//! SQL tables: a project's, which the agent creates and queries, held by the
//! backend its [`crate::model::store::Store`] picks, or an app's own, on this
//! machine at a [`Home`].

pub use artifact::table::{
    ColType, Column, Home, Local, Reply, Table, Tables,
    rows::{Edit, Page, Row, Rows},
};
pub use call::Calls;

pub(crate) const FILE: &str = artifact::project::fs::DATA;

/// A database of tables, on whichever backend holds it.
pub type Data = Box<dyn Tables>;

/// Where this build keeps tables on this machine.
#[cfg(feature = "desktop")]
pub type Here = artifact::table::sqlite::Sqlite;
#[cfg(not(feature = "desktop"))]
pub type Here = none::Absent;

mod call;
#[cfg(not(feature = "desktop"))]
mod none;
