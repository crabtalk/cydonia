//! The `tables` Cupola service: [`Tables`](super::Tables) calls on the tables
//! of a project another machine holds. A server answers them with the
//! [`Sqlite`](super::sqlite::Sqlite) store at the root each request names;
//! [`super::remote`] is the client's side.
//!
//! Cells cross in their JSON.

use super::{
    ColType, Column, Table,
    rows::{Edit, Page, Rows},
};
use crate::wire::{Refusal, json};
use cupola::Service;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// A project's tables, called over a connection.
pub struct Tables;

/// One call on the tables of the project at `root`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    #[serde(with = "cupola::bytes::path")]
    pub root: PathBuf,
    pub call: Call,
}

/// The [`Tables`](super::Tables) calls, and what each is answered with.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Call {
    /// Answered with [`Answer::Tables`].
    List,
    /// Answered with [`Answer::Done`].
    Archive { key: String, archived: bool },
    /// Answered with [`Answer::Done`].
    Touch(String),
    /// Answered with [`Answer::Table`].
    Create {
        name: String,
        key: Option<String>,
        columns: Vec<Column>,
        author: Option<String>,
    },
    /// Answered with [`Answer::Table`].
    Update {
        table: String,
        name: Option<String>,
        key: Option<String>,
    },
    /// Answered with [`Answer::Done`].
    Remove(String),
    /// Answered with [`Answer::Columns`].
    WriteColumn {
        table: String,
        column: String,
        kind: Option<ColType>,
        rename: Option<String>,
    },
    /// Answered with [`Answer::Page`].
    Read {
        table: String,
        sort: Option<String>,
        desc: bool,
        limit: i64,
        offset: i64,
    },
    /// Answered with [`Answer::Done`].
    WriteCells {
        table: String,
        #[serde(with = "json")]
        edits: Vec<Edit>,
    },
    /// Answered with [`Answer::Ids`].
    AddRows { table: String, count: i64 },
    /// Answered with [`Answer::Ids`].
    Insert {
        table: String,
        #[serde(with = "json")]
        rows: Vec<Map<String, Value>>,
    },
    /// Answered with [`Answer::Done`].
    DeleteWhere {
        table: String,
        column: String,
        #[serde(with = "json")]
        value: Value,
    },
    /// Answered with [`Answer::Done`].
    DeleteRows { table: String, rowids: Vec<i64> },
    /// Answered with [`Answer::Columns`].
    DropColumn { table: String, column: String },
    /// Answered with [`Answer::Rows`].
    Query(String),
    /// Answered with [`Answer::Count`].
    Write(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Answer {
    Tables(Vec<Table>),
    Table(Table),
    Columns(Vec<Column>),
    Page(#[serde(with = "json")] Page),
    Rows(#[serde(with = "json")] Rows),
    Ids(Vec<i64>),
    Count(u64),
    Done,
}

impl Service for Tables {
    const NAME: &'static str = "tables";
    type Request = Request;
    type Response = Result<Answer, Refusal>;
    type Event = ();
}

#[cfg(feature = "sqlite")]
mod server {
    use super::{Answer, Call, Request, Tables};
    use crate::table::{Home, Local as _, sqlite::Sqlite};
    use crate::wire::Refusal;
    use anyhow::{Result, anyhow};
    use cupola::{Server, lanes::Lanes};
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    /// The store at a root: `None` until a call finds or makes one.
    type Slot = Arc<Mutex<Option<Sqlite>>>;

    /// The server's stores, one per root asked about.
    #[derive(Default)]
    struct Stores {
        held: Mutex<HashMap<PathBuf, Slot>>,
    }

    impl Tables {
        /// Answers [`Tables`] requests on `server`, on one [`Lanes`] lane per
        /// root, so one root's requests land in the order they were sent. A
        /// read attaches the store a root already has and answers as an empty
        /// one when there is none; a write makes the store.
        pub fn register(server: &Server) {
            let stores = Arc::new(Stores::default());
            let lanes = Lanes::default();
            server.register::<Self, _, _>(move |request: Request| {
                let stores = stores.clone();
                lanes.run(request.root.clone(), move || {
                    let slot = stores.slot(&request.root);
                    let home = Home::Project(request.root);
                    request
                        .call
                        .answer(
                            &mut slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
                            &home,
                        )
                        .map_err(Refusal::from)
                })
            });
        }
    }

    impl Stores {
        fn slot(&self, root: &Path) -> Slot {
            self.held
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .entry(root.to_owned())
                .or_default()
                .clone()
        }
    }

    impl Call {
        fn answer(self, slot: &mut Option<Sqlite>, home: &Home) -> Result<Answer> {
            if slot.is_none() {
                *slot = match self.writes() {
                    true => Some(Sqlite::open(home)?),
                    false => Sqlite::attach(home),
                };
            }
            let Some(store) = slot else {
                return self.unheld();
            };
            Ok(match self {
                Self::List => Answer::Tables(store.list()?),
                Self::Archive { key, archived } => {
                    store.archive(&key, archived)?;
                    Answer::Done
                }
                Self::Touch(key) => {
                    store.touch(&key)?;
                    Answer::Done
                }
                Self::Create {
                    name,
                    key,
                    columns,
                    author,
                } => Answer::Table(store.create(
                    &name,
                    key.as_deref(),
                    &columns,
                    author.as_deref(),
                )?),
                Self::Update { table, name, key } => {
                    Answer::Table(store.update(&table, name.as_deref(), key.as_deref())?)
                }
                Self::Remove(table) => {
                    store.remove(&table)?;
                    Answer::Done
                }
                Self::WriteColumn {
                    table,
                    column,
                    kind,
                    rename,
                } => {
                    Answer::Columns(store.write_column(&table, &column, kind, rename.as_deref())?)
                }
                Self::Read {
                    table,
                    sort,
                    desc,
                    limit,
                    offset,
                } => Answer::Page(store.read(&table, sort.as_deref(), desc, limit, offset)?),
                Self::WriteCells { table, edits } => {
                    store.write_cells(&table, &edits)?;
                    Answer::Done
                }
                Self::AddRows { table, count } => Answer::Ids(store.add_rows(&table, count)?),
                Self::Insert { table, rows } => Answer::Ids(store.insert(&table, &rows)?),
                Self::DeleteWhere {
                    table,
                    column,
                    value,
                } => {
                    store.delete_where(&table, &column, &value)?;
                    Answer::Done
                }
                Self::DeleteRows { table, rowids } => {
                    store.delete_rows(&table, &rowids)?;
                    Answer::Done
                }
                Self::DropColumn { table, column } => {
                    Answer::Columns(store.drop_column(&table, &column)?)
                }
                Self::Query(sql) => Answer::Rows(store.query(&sql)?),
                Self::Write(sql) => Answer::Count(store.write(&sql)?),
            })
        }

        /// Whether the call writes, and so makes the store when there is
        /// none.
        fn writes(&self) -> bool {
            !matches!(self, Self::List | Self::Read { .. } | Self::Query(_))
        }

        /// A read's answer from a root with no store.
        fn unheld(self) -> Result<Answer> {
            match self {
                Self::List => Ok(Answer::Tables(Vec::new())),
                Self::Read { table, .. } => Err(anyhow!("no table {table}")),
                _ => Err(anyhow!("the project has no tables")),
            }
        }
    }
}
