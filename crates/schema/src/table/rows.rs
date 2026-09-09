//! What is in a table: a window of it, one row, and one cell to write.
//!
//! Everything here addresses a row by its `rowid`, SQLite's own identity for
//! it — a handle that survives a sort, an insert above it and a neighbour's
//! deletion, with no surrogate key column to hide from the person and no
//! position index that means something different the moment the order changes.

use crate::table::Column;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A `SELECT`'s answer: the column names once, then the rows.
#[derive(Debug, Serialize)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// One window of a table, with everything a grid needs to draw it.
#[derive(Debug, Serialize)]
pub struct Page {
    pub key: String,
    pub name: String,
    pub columns: Vec<Column>,
    /// Rows in the whole table, not in this window — the scrollbar's length.
    pub total: i64,
    /// Where `rows` starts, echoed back so a window that arrives after the
    /// viewport moved on can be dropped rather than drawn in the wrong place.
    pub offset: i64,
    pub rows: Vec<Row>,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub rowid: i64,
    /// In `columns` order.
    pub cells: Vec<Value>,
}

/// One cell to write. A pasted block and a single edit are the same thing at
/// different lengths, so there is one shape for both.
#[derive(Debug, Deserialize)]
pub struct Edit {
    pub rowid: i64,
    pub column: String,
    pub value: Value,
}
