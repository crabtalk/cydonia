//! Stable, project-wide numbers alongside each entry's storage identity.

use crate::kind::Kind;
use crate::project::Project;
#[cfg(feature = "sqlite")]
use crate::project::fs;
use anyhow::Result;
#[cfg(feature = "sqlite")]
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
#[cfg(feature = "sqlite")]
use std::path::Path;
#[cfg(feature = "sqlite")]
use std::time::Duration;

#[cfg(feature = "sqlite")]
pub struct Registry(Connection);

#[cfg(feature = "sqlite")]
impl Registry {
    pub fn open(project: &Path) -> Result<Self> {
        let path = fs::Project::new(project).state()?;
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                number INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                id TEXT,
                UNIQUE(kind, id)
            )",
        )?;
        Ok(Self(connection))
    }

    pub fn number(&mut self, kind: Kind, id: &str) -> Result<u64> {
        if let Some(number) = self
            .0
            .query_row(
                "SELECT number FROM entries WHERE kind = ?1 AND id = ?2",
                [kind.key(), id],
                |row| row.get::<_, i64>(0).map(|number| number as u64),
            )
            .optional()?
        {
            return Ok(number);
        }
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO entries (kind, id) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            [kind.key(), id],
        )?;
        let number = tx.query_row(
            "SELECT number FROM entries WHERE kind = ?1 AND id = ?2",
            [kind.key(), id],
            |row| row.get::<_, i64>(0).map(|number| number as u64),
        )?;
        tx.commit()?;
        Ok(number)
    }

    pub fn resolve(&self, kind: Kind, number: u64) -> Result<Option<String>> {
        Ok(self
            .0
            .query_row(
                "SELECT id FROM entries WHERE kind = ?1 AND number = ?2 AND id IS NOT NULL",
                params![kind.key(), i64::try_from(number)?],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Every number issued and not retired, with what it is on, in number
    /// order. A row whose kind this build does not know is skipped.
    pub fn issued(&self) -> Result<Vec<(u64, Kind, String)>> {
        let mut statement = self
            .0
            .prepare("SELECT number, kind, id FROM entries WHERE id IS NOT NULL ORDER BY number")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut issued = Vec::new();
        for row in rows {
            let (number, kind, id) = row?;
            if let Some(kind) = Kind::from_key(&kind) {
                issued.push((number, kind, id));
            }
        }
        Ok(issued)
    }

    pub fn rename(&self, kind: Kind, old: &str, new: &str) -> Result<()> {
        self.0.execute(
            "UPDATE entries SET id = ?3 WHERE kind = ?1 AND id = ?2",
            [kind.key(), old, new],
        )?;
        Ok(())
    }

    /// Retain the number as a tombstone so a recreated key gets a fresh one.
    pub fn remove(&self, kind: Kind, id: &str) -> Result<()> {
        self.0.execute(
            "UPDATE entries SET id = NULL WHERE kind = ?1 AND id = ?2",
            [kind.key(), id],
        )?;
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
pub fn number(project: &Path, kind: Kind, id: &str) -> Result<u64> {
    Registry::open(project)?.number(kind, id)
}

pub fn label(number: Option<u64>, title: &str) -> String {
    match number {
        Some(number) => format!("#{number} {title}"),
        None => title.to_owned(),
    }
}

#[derive(serde::Serialize)]
pub struct Entry {
    pub number: u64,
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub archived: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// For an article's own session, the article's number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article: Option<u64>,
}

/// A backend's boards, articles and sessions as entries, in no order.
pub fn catalog(store: &impl Project) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for board in store.boards() {
        entries.push(Entry {
            number: numbered(board.number, Kind::Board, &board.id)?,
            title: board.label().to_owned(),
            kind: Kind::Board,
            id: board.id,
            archived: board.archived,
            labels: board.labels,
            article: None,
        });
    }
    for article in store.articles() {
        entries.push(Entry {
            number: numbered(article.number, Kind::Article, &article.id)?,
            kind: Kind::Article,
            id: article.id,
            title: article.title,
            archived: article.archived,
            labels: article.labels,
            article: None,
        });
    }
    for session in store.sessions() {
        let article = session
            .article
            .as_deref()
            .and_then(|article| store.article(article))
            .and_then(|article| article.number);
        entries.push(Entry {
            number: numbered(session.number, Kind::Session, &session.id)?,
            kind: Kind::Session,
            title: session.label().to_owned(),
            id: session.id,
            archived: session.closed,
            labels: session.labels,
            article,
        });
    }
    Ok(entries)
}

/// The number a read handed an entry, or why there is none.
fn numbered(number: Option<u64>, kind: Kind, id: &str) -> Result<u64> {
    number.ok_or_else(|| anyhow::anyhow!("no number for {kind} {id}"))
}

/// One of [`catalog`]'s entries as a client reads it. `None` for a kind the
/// backend does not hold.
pub fn open(store: &impl Project, entry: &Entry) -> Result<Option<serde_json::Value>> {
    Ok(Some(match entry.kind {
        Kind::Article => serde_json::json!({"markdown": store.read_article(&entry.id)?}),
        Kind::Board => serde_json::to_value(
            store
                .board(&entry.id)
                .ok_or_else(|| anyhow::anyhow!("board no longer exists"))?,
        )?,
        Kind::Session => serde_json::to_value(
            store
                .session(&entry.id)
                .ok_or_else(|| anyhow::anyhow!("session no longer exists"))?,
        )?,
        _ => return Ok(None),
    }))
}

/// Discover existing content without creating storage in an empty project:
/// the [`catalog`] of its files, and the tables in its database.
#[cfg(feature = "sqlite")]
pub fn list(project: &Path) -> Result<Vec<Entry>> {
    let store = fs::Project::new(project);
    let mut entries = catalog(&store)?;
    if store.cydonia().join(fs::DATA).is_file() {
        let connection = data(project)?;
        let mut query = connection.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_tables' ORDER BY name",
        )?;
        for id in query.query_map([], |row| row.get::<_, String>(0))? {
            let id = id?;
            let (title, archived) = connection
                .query_row(
                    "SELECT name, COALESCE(archived, 0) FROM _tables WHERE key = ?1",
                    [&id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or_else(|_| (id.clone(), false));
            entries.push(Entry {
                number: number(project, Kind::Table, &id)?,
                kind: Kind::Table,
                id,
                title,
                archived,
                labels: Vec::new(),
                article: None,
            });
        }
    }
    entries.sort_by_key(|entry| entry.number);
    Ok(entries)
}

#[cfg(feature = "sqlite")]
fn data(project: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        fs::Project::new(project).cydonia().join(fs::DATA),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    Ok(connection)
}

/// Read the catalog's entry; table previews contain at most 200 rows.
#[cfg(feature = "sqlite")]
pub fn read(project: &Path, entry: &Entry) -> Result<serde_json::Value> {
    let store = fs::Project::new(project);
    if let Some(value) = open(&store, entry)? {
        return Ok(value);
    }
    Ok(match entry.kind {
        Kind::Table => {
            use rusqlite::types::ValueRef;
            let connection = data(project)?;
            let key = crate::table::quote(&entry.id);
            let total: i64 =
                connection
                    .query_row(&format!("SELECT count(*) FROM {key}"), [], |row| row.get(0))?;
            let mut query = connection.prepare(&format!("SELECT * FROM {key} LIMIT 200"))?;
            let columns: Vec<String> = query
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            let rows = query
                .query_map([], |row| {
                    (0..columns.len())
                        .map(|ix| {
                            Ok(match row.get_ref(ix)? {
                                ValueRef::Null => serde_json::Value::Null,
                                ValueRef::Integer(value) => value.into(),
                                ValueRef::Real(value) => serde_json::json!(value),
                                ValueRef::Text(value) => {
                                    String::from_utf8_lossy(value).into_owned().into()
                                }
                                ValueRef::Blob(value) => {
                                    serde_json::json!({"blob_bytes": value.len()})
                                }
                            })
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            serde_json::json!({"key": entry.id, "columns": columns, "rows": rows, "total": total, "limit": 200})
        }
        _ => anyhow::bail!("unknown entry kind {}", entry.kind),
    })
}
