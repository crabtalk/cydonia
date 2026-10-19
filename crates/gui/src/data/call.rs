//! An app's calls on the store, in the shapes `cydonia-guest` reads.

use super::{ColType, Column, Table, Tables};
use anyhow::{Result, bail};
use guest::wire::{self, Answer, Call};

/// [`Tables`] as an app calls them.
pub trait Calls {
    /// Do an app's call. A call that writes touches the table it wrote.
    fn call(&mut self, call: Call) -> Result<Answer>;
}

impl<T: Tables + ?Sized> Calls for T {
    fn call(&mut self, call: Call) -> Result<Answer> {
        Ok(match call {
            Call::Tables => Answer::Tables(self.list()?.iter().map(Wire::wire).collect()),
            Call::Query { sql } => {
                let rows = self.query(&sql)?;
                Answer::Rows(wire::Rows {
                    columns: rows.columns,
                    rows: rows.rows,
                })
            }
            Call::CreateTable { name, columns } => {
                let columns: Vec<Column> = columns
                    .into_iter()
                    .map(|column| Column {
                        name: column.name,
                        kind: column.kind.wire(),
                    })
                    .collect();
                Answer::Table(self.create(&name, None, &columns, None)?.wire())
            }
            Call::Insert { table, rows } => {
                let ids = self.insert(&table, &rows)?;
                let _ = self.touch(&table);
                Answer::Ids(ids)
            }
            Call::Delete {
                table,
                column,
                value,
            } => {
                self.delete_where(&table, &column, &value)?;
                let _ = self.touch(&table);
                Answer::Done
            }
            Call::Fetch(request) => bail!("{} is a fetch, not a call on tables", request.url),
        })
    }
}

/// A store shape as an app reads it, and back.
trait Wire {
    type Other;

    fn wire(&self) -> Self::Other;
}

impl Wire for Table {
    type Other = wire::Table;

    fn wire(&self) -> wire::Table {
        wire::Table {
            key: self.key.clone(),
            name: self.name.clone(),
            number: self.number,
            columns: self
                .columns
                .iter()
                .map(|column| wire::Column {
                    name: column.name.clone(),
                    kind: column.kind.wire(),
                })
                .collect(),
            rows: self.rows,
        }
    }
}

impl Wire for ColType {
    type Other = wire::ColType;

    fn wire(&self) -> wire::ColType {
        match self {
            ColType::Text => wire::ColType::Text,
            ColType::Number => wire::ColType::Number,
            ColType::Date => wire::ColType::Date,
            ColType::Check => wire::ColType::Check,
        }
    }
}

impl Wire for wire::ColType {
    type Other = ColType;

    fn wire(&self) -> ColType {
        match self {
            wire::ColType::Text => ColType::Text,
            wire::ColType::Number => ColType::Number,
            wire::ColType::Date => ColType::Date,
            wire::ColType::Check => ColType::Check,
        }
    }
}
