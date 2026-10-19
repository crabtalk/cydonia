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
