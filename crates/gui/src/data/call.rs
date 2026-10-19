//! An app's calls on the store, in the shapes `cydonia-guest` reads.

use super::{ColType, Column, Reply, Table, Tables};
use anyhow::anyhow;
use futures::{FutureExt as _, TryFutureExt as _, future};
use guest::wire::{self, Answer, Call};

/// [`Tables`] as an app calls them.
pub trait Calls {
    /// Do an app's call. A call that writes touches the table it wrote.
    fn call(&mut self, call: Call) -> Reply<Answer>;
}

impl<T: Tables + ?Sized> Calls for T {
    fn call(&mut self, call: Call) -> Reply<Answer> {
        match call {
            Call::Tables => self
                .list()
                .map_ok(|tables| Answer::Tables(tables.iter().map(Wire::wire).collect()))
                .boxed(),
            Call::Query { sql } => self
                .query(&sql)
                .map_ok(|rows| {
                    Answer::Rows(wire::Rows {
                        columns: rows.columns,
                        rows: rows.rows,
                    })
                })
                .boxed(),
            Call::CreateTable { name, columns } => {
                let columns: Vec<Column> = columns
                    .into_iter()
                    .map(|column| Column {
                        name: column.name,
                        kind: column.kind.wire(),
                    })
                    .collect();
                self.create(&name, None, &columns, None)
                    .map_ok(|table| Answer::Table(table.wire()))
                    .boxed()
            }
            Call::Insert { table, rows } => {
                let inserted = self.insert(&table, &rows);
                let touched = self.touch(&table);
                async move {
                    let ids = inserted.await?;
                    let _ = touched.await;
                    Ok(Answer::Ids(ids))
                }
                .boxed()
            }
            Call::Delete {
                table,
                column,
                value,
            } => {
                let deleted = self.delete_where(&table, &column, &value);
                let touched = self.touch(&table);
                async move {
                    deleted.await?;
                    let _ = touched.await;
                    Ok(Answer::Done)
                }
                .boxed()
            }
            Call::Fetch(request) => future::ready(Err(anyhow!(
                "{} is a fetch, not a call on tables",
                request.url
            )))
            .boxed(),
        }
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
