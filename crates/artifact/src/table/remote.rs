//! The tables of a project another machine holds, reached through the
//! [`service::Tables`] service. Every call is a request; its future resolves
//! when the server answers. Table numbers are the server's.

use super::{
    ColType, Column, Reply, Table,
    rows::{Edit, Page, Rows},
    service::{self, Answer, Call, Request},
};
use crate::error::Error;
use anyhow::anyhow;
use cupola::{Client, Handle};
use futures::FutureExt as _;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The tables of the project at one root on one server.
#[derive(Clone)]
pub struct Tables {
    root: PathBuf,
    handle: Handle<service::Tables>,
}

impl Tables {
    /// The tables of the project at `root` on the server `client` reaches.
    pub fn new(client: &Client, root: PathBuf) -> Self {
        Self {
            root,
            handle: client.service::<service::Tables>(),
        }
    }

    /// The project's directory on the server's machine.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Sends `call`, and answers what `take` makes of the server's answer.
    /// A refusal fails as the [`Error`] it carries.
    fn call<T: Send + 'static>(
        &self,
        call: Call,
        take: impl FnOnce(Answer) -> Option<T> + Send + 'static,
    ) -> Reply<T> {
        let answer = self.handle.send(&Request {
            root: self.root.clone(),
            call,
        });
        async move {
            let answer = answer.await?.map_err(Error::from)?;
            take(answer).ok_or_else(|| anyhow!("the server answered another call"))
        }
        .boxed()
    }

    fn done(&self, call: Call) -> Reply<()> {
        self.call(call, |answer| matches!(answer, Answer::Done).then_some(()))
    }

    fn table(&self, call: Call) -> Reply<Table> {
        self.call(call, |answer| match answer {
            Answer::Table(table) => Some(table),
            _ => None,
        })
    }

    fn columns(&self, call: Call) -> Reply<Vec<Column>> {
        self.call(call, |answer| match answer {
            Answer::Columns(columns) => Some(columns),
            _ => None,
        })
    }

    fn ids(&self, call: Call) -> Reply<Vec<i64>> {
        self.call(call, |answer| match answer {
            Answer::Ids(ids) => Some(ids),
            _ => None,
        })
    }
}

impl super::Tables for Tables {
    fn list(&self) -> Reply<Vec<Table>> {
        self.call(Call::List, |answer| match answer {
            Answer::Tables(tables) => Some(tables),
            _ => None,
        })
    }

    fn archive(&mut self, key: &str, archived: bool) -> Reply<()> {
        self.done(Call::Archive {
            key: key.to_owned(),
            archived,
        })
    }

    fn touch(&mut self, key: &str) -> Reply<()> {
        self.done(Call::Touch(key.to_owned()))
    }

    fn create(
        &mut self,
        name: &str,
        key: Option<&str>,
        columns: &[Column],
        author: Option<&str>,
    ) -> Reply<Table> {
        self.table(Call::Create {
            name: name.to_owned(),
            key: key.map(str::to_owned),
            columns: columns.to_vec(),
            author: author.map(str::to_owned),
        })
    }

    fn update(&mut self, table: &str, name: Option<&str>, key: Option<&str>) -> Reply<Table> {
        self.table(Call::Update {
            table: table.to_owned(),
            name: name.map(str::to_owned),
            key: key.map(str::to_owned),
        })
    }

    fn remove(&mut self, table: &str) -> Reply<()> {
        self.done(Call::Remove(table.to_owned()))
    }

    fn write_column(
        &mut self,
        table: &str,
        column: &str,
        kind: Option<ColType>,
        rename: Option<&str>,
    ) -> Reply<Vec<Column>> {
        self.columns(Call::WriteColumn {
            table: table.to_owned(),
            column: column.to_owned(),
            kind,
            rename: rename.map(str::to_owned),
        })
    }

    fn read(
        &self,
        table: &str,
        sort: Option<&str>,
        desc: bool,
        limit: i64,
        offset: i64,
    ) -> Reply<Page> {
        let call = Call::Read {
            table: table.to_owned(),
            sort: sort.map(str::to_owned),
            desc,
            limit,
            offset,
        };
        self.call(call, |answer| match answer {
            Answer::Page(page) => Some(page),
            _ => None,
        })
    }

    fn write_cells(&mut self, table: &str, edits: &[Edit]) -> Reply<()> {
        self.done(Call::WriteCells {
            table: table.to_owned(),
            edits: edits.to_vec(),
        })
    }

    fn add_rows(&mut self, table: &str, count: i64) -> Reply<Vec<i64>> {
        self.ids(Call::AddRows {
            table: table.to_owned(),
            count,
        })
    }

    fn insert(&mut self, table: &str, rows: &[Map<String, Value>]) -> Reply<Vec<i64>> {
        self.ids(Call::Insert {
            table: table.to_owned(),
            rows: rows.to_vec(),
        })
    }

    fn delete_where(&mut self, table: &str, column: &str, value: &Value) -> Reply<()> {
        self.done(Call::DeleteWhere {
            table: table.to_owned(),
            column: column.to_owned(),
            value: value.clone(),
        })
    }

    fn delete_rows(&mut self, table: &str, rowids: &[i64]) -> Reply<()> {
        self.done(Call::DeleteRows {
            table: table.to_owned(),
            rowids: rowids.to_vec(),
        })
    }

    fn drop_column(&mut self, table: &str, column: &str) -> Reply<Vec<Column>> {
        self.columns(Call::DropColumn {
            table: table.to_owned(),
            column: column.to_owned(),
        })
    }

    fn query(&self, sql: &str) -> Reply<Rows> {
        self.call(Call::Query(sql.to_owned()), |answer| match answer {
            Answer::Rows(rows) => Some(rows),
            _ => None,
        })
    }

    fn write(&mut self, sql: &str) -> Reply<u64> {
        self.call(Call::Write(sql.to_owned()), |answer| match answer {
            Answer::Count(count) => Some(count),
            _ => None,
        })
    }
}
