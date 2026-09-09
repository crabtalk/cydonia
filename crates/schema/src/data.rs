//! The project's tables, as shapes: what a column may hold, what a table is,
//! and what a read of one comes back as.
//!
//! No SQL here and no connection — those are the backend's, and the backend is
//! whichever store the app opened. What is here is the part a client reads: a
//! table it was told about and a page of rows it was handed have the same
//! shape whether they came out of SQLite or off a wire.
//!
//! A table's `key` is the SQL identifier and never moves; its `name` is free
//! text, so renaming one cannot break a query an agent already wrote.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone, Serialize)]
pub struct Table {
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
    pub rows: Vec<Record>,
}

#[derive(Debug, Serialize)]
pub struct Record {
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
