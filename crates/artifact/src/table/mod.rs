//! The project's tables: what a column may hold, and what a table is.
//! [`rows`] holds what is in one.
//!
//! A table's `key` is its SQL identifier; its `name` is display text.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod rows;
pub mod store;

/// The declared type is stored in the SQLite catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColType {
    Text,
    Number,
    Date,
    Check,
    Url,
}

impl ColType {
    pub const ALL: [Self; 5] = [Self::Text, Self::Number, Self::Date, Self::Check, Self::Url];

    /// The wire name — what the tools take and hand back.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::Check => "check",
            Self::Url => "url",
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
            Self::Url => "URL TEXT",
        }
    }
}

/// Reading the catalog back. Total rather than fallible, because the catalog is
/// not our enum: a table made outside these tools may declare anything, and a
/// column we cannot name is still a column the person can read.
impl From<&str> for ColType {
    fn from(declared: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| declared.eq_ignore_ascii_case(kind.sql()))
            .unwrap_or(Self::Text)
    }
}

/// A column's name is its SQL identifier — quoted everywhere, so it stays free
/// text the way a spreadsheet header is. Only tables carry a key, because only
/// a table is addressed in prose after it is renamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ColType,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColumnDisplay {
    pub position: Option<usize>,
    pub width: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnLayout {
    pub settings: BTreeMap<String, ColumnDisplay>,
}

impl ColumnLayout {
    pub fn order(&self, columns: &[Column]) -> Vec<usize> {
        let mut order: Vec<_> = (0..columns.len()).collect();
        order.sort_by_key(|&ix| {
            self.settings
                .get(&columns[ix].name)
                .and_then(|display| display.position)
                .unwrap_or(usize::MAX)
        });
        order
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Table {
    pub number: Option<u64>,
    /// What `FROM` takes.
    pub key: String,
    pub name: String,
    pub columns: Vec<Column>,
    #[serde(skip)]
    pub column_layout: ColumnLayout,
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
