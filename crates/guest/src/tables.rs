//! The app's own tables, kept for it on this machine whichever project is in
//! front. Each call answers later, as the message `msg` makes of its result.
//!
//! ```ignore
//! tables::query("SELECT name FROM companies", Msg::Loaded);
//! ```

use crate::{
    host,
    wire::{Answer, Call, ColType, Column, Rows, Table},
};
use serde_json::{Map, Value};

/// A row to insert, by column name.
pub type Record = Map<String, Value>;

/// Every table in the project.
pub fn list<M: 'static>(msg: impl FnOnce(Result<Vec<Table>, String>) -> M + 'static) {
    host::request(&Call::Tables, move |answer| {
        msg(match answer {
            Answer::Tables(tables) => Ok(tables),
            other => Err(unexpected(other)),
        })
    });
}

/// Run a read-only SQL statement. Tables are named by their key.
pub fn query<M: 'static>(
    sql: impl Into<String>,
    msg: impl FnOnce(Result<Rows, String>) -> M + 'static,
) {
    let sql = sql.into();
    host::request(&Call::Query { sql }, move |answer| {
        msg(match answer {
            Answer::Rows(rows) => Ok(rows),
            other => Err(unexpected(other)),
        })
    });
}

/// Make a table named `name`. Its key, what SQL names it by, is derived from
/// the name and answered in the [`Table`].
pub fn create<M: 'static>(
    name: impl Into<String>,
    columns: impl IntoIterator<Item = (impl Into<String>, ColType)>,
    msg: impl FnOnce(Result<Table, String>) -> M + 'static,
) {
    let call = Call::CreateTable {
        name: name.into(),
        columns: columns
            .into_iter()
            .map(|(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    };
    host::request(&call, move |answer| {
        msg(match answer {
            Answer::Table(table) => Ok(table),
            other => Err(unexpected(other)),
        })
    });
}

/// Add `rows` to the table `table`, all or none, answering with their ids.
pub fn insert<M: 'static>(
    table: impl Into<String>,
    rows: Vec<Record>,
    msg: impl FnOnce(Result<Vec<i64>, String>) -> M + 'static,
) {
    let call = Call::Insert {
        table: table.into(),
        rows,
    };
    host::request(&call, move |answer| {
        msg(match answer {
            Answer::Ids(ids) => Ok(ids),
            other => Err(unexpected(other)),
        })
    });
}

/// Take away the rows of `table` whose `column` holds `value`.
pub fn delete<M: 'static>(
    table: impl Into<String>,
    column: impl Into<String>,
    value: impl Into<Value>,
    msg: impl FnOnce(Result<(), String>) -> M + 'static,
) {
    let call = Call::Delete {
        table: table.into(),
        column: column.into(),
        value: value.into(),
    };
    host::request(&call, move |answer| {
        msg(match answer {
            Answer::Done => Ok(()),
            other => Err(unexpected(other)),
        })
    });
}

fn unexpected(answer: Answer) -> String {
    match answer {
        Answer::Error(error) => error,
        other => format!("Cydonia answered {other:?}"),
    }
}
