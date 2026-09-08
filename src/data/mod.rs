//! The project's database: SQL tables the agent creates and queries, in a file
//! of its own under the project's `.cydonia/`.
//!
//! SQLite's catalog is the registry — columns are read back out of `PRAGMA
//! table_info`, so there is no second list of them to fall out of step with the
//! first, which is the drift that rots this kind of feature. A table's `key` is
//! the SQL identifier and never moves; its `name` is free text, so renaming one
//! cannot break a query the agent already wrote. Reads go to a handle opened
//! read-only: what stops a `SELECT` writing is the connection itself, never
//! anything believed about the text.

use crate::model::project;
use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OpenFlags, Row, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod ddl;
mod rows;
mod sql;

pub use rows::{Edit, Page, Record};

pub(crate) const FILE: &str = "data.db";
const BUSY: Duration = Duration::from_secs(5);

/// The one thing stored beside the data: a table's display name, which SQL has
/// nowhere to keep. Losing a row here costs a table its name, never its rows.
const DDL: &str = "CREATE TABLE IF NOT EXISTS _tables (
    key        TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER,
    archived   INTEGER,
    author     TEXT
)";

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
    fn sql(self) -> &'static str {
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

pub struct Data {
    writer: Connection,
    /// Opened `SQLITE_OPEN_READ_ONLY`, and that is the point: a `SELECT` an
    /// agent wrote runs on a connection that cannot write, whatever the
    /// statement turns out to say. A guard on the text would be a rule the next
    /// parser bug walks past.
    reader: Connection,
}

impl Data {
    /// Open the project's store: the writer, and a read-only handle beside it.
    ///
    /// Order is load-bearing. The reader cannot create the file, and on a WAL
    /// database it needs the shared-memory sidecar an open writer has already
    /// made — so opening it first fails on a store nobody has written yet.
    pub fn open(project: &Path) -> Result<Self> {
        let path = project::init(project)?.join(FILE);

        let writer = Connection::open(&path)?;
        writer.busy_timeout(BUSY)?;
        writer.execute_batch("PRAGMA journal_mode = WAL")?;
        writer.execute_batch(DDL)?;
        // A `_tables` written before a column existed. SQLite has no ADD COLUMN
        // IF NOT EXISTS, and the file is the only record of which shape this
        // one is.
        let held = columns_of(&writer, "_tables")?;
        for column in ["updated_at", "archived"] {
            if !held.iter().any(|col| col.name == column) {
                writer
                    .execute_batch(&format!("ALTER TABLE _tables ADD COLUMN {column} INTEGER"))?;
            }
        }

        let reader = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reader.busy_timeout(BUSY)?;
        Ok(Self { writer, reader })
    }

    /// The store as it already is, or nothing. Browsing a project must not
    /// write a database into it — [`Data::open`] is what creates one.
    pub fn attach(project: &Path) -> Option<Self> {
        match project::dir(project).join(FILE).exists() {
            true => Self::open(project).ok(),
            false => None,
        }
    }

    /// Every table with its columns and row count, in [`Data::keys`]'s order.
    pub fn list(&self) -> Result<Vec<Table>> {
        self.keys()?.iter().map(|key| self.table(key)).collect()
    }

    /// Put a table away, or bring it back. The rows stay exactly where they
    /// are — this is the sidebar's business and nothing else's.
    pub fn archive(&mut self, key: &str, archived: bool) -> Result<()> {
        self.writer.execute(
            "UPDATE _tables SET archived = ?2 WHERE key = ?1",
            rusqlite::params![key, archived],
        )?;
        Ok(())
    }

    /// Mark a table written just now — the stamp [`Data::keys`] orders on, so
    /// the one being worked in is the one on top.
    pub fn touch(&mut self, key: &str) -> Result<()> {
        self.writer.execute(
            "UPDATE _tables SET updated_at = ?2 WHERE key = ?1",
            rusqlite::params![key, now()],
        )?;
        Ok(())
    }

    /// One table by its key, built from the catalog outward rather than from
    /// `_tables`: the data is what exists, and a table whose name row went
    /// missing must still list — under its key — instead of vanishing from a
    /// surface that is supposed to show everything in the file.
    fn table(&self, key: &str) -> Result<Table> {
        let meta = self
            .reader
            .query_row(
                "SELECT name, created_at, updated_at, archived, author FROM _tables \
                 WHERE key = ?1",
                [key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get::<_, Option<bool>>(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .ok();
        let (name, created_at, updated_at, archived, author) = meta.unwrap_or_else(|| {
            (
                key.to_owned(),
                0i64,
                None::<i64>,
                None::<bool>,
                None::<String>,
            )
        });
        Ok(Table {
            columns: columns_of(&self.reader, key)?,
            rows: self.reader.query_row(
                &format!("SELECT count(*) FROM {}", quote(key)),
                [],
                |row| row.get(0),
            )?,
            key: key.to_owned(),
            name,
            created_at,
            updated_at,
            archived: archived.unwrap_or_default(),
            author,
        })
    }

    /// The tables in the file — everything SQLite holds that is not its own
    /// bookkeeping or ours — last touched first, which is how an article and a
    /// board are listed. The catalog is still what says a table exists: one
    /// with no `_tables` row has no age to sort on and goes last, under its
    /// key.
    fn keys(&self) -> Result<Vec<String>> {
        let mut stmt = self.reader.prepare(
            "SELECT m.name FROM sqlite_master m \
             LEFT JOIN _tables t ON t.key = m.name \
             WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' AND m.name <> '_tables' \
             ORDER BY COALESCE(t.updated_at, t.created_at, 0) DESC, m.name",
        )?;
        let keys = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(keys)
    }

    /// `key` overrides the identifier derived from the name — the one chance to
    /// choose it, since afterwards it is what every written query spells.
    pub fn create(
        &mut self,
        name: &str,
        key: Option<&str>,
        columns: &[Column],
        author: Option<&str>,
    ) -> Result<Table> {
        let name = name.trim();
        if name.is_empty() {
            bail!("a table needs a name");
        }
        if columns.is_empty() {
            bail!("a table needs at least one column");
        }
        let mut seen = HashSet::new();
        for col in columns {
            let column = col.name.trim();
            if column.is_empty() {
                bail!("a column needs a name");
            }
            if !seen.insert(column.to_ascii_lowercase()) {
                bail!("two columns named {column}");
            }
        }
        let key = self.mint_key(key.filter(|k| !k.trim().is_empty()).unwrap_or(name))?;
        let defs = columns
            .iter()
            .map(|col| format!("{} {}", quote(col.name.trim()), col.kind.sql()))
            .collect::<Vec<_>>()
            .join(", ");

        let tx = self.writer.transaction()?;
        tx.execute_batch(&format!("CREATE TABLE {} ({defs})", quote(&key)))?;
        tx.execute(
            "INSERT INTO _tables (key, name, created_at, author) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&key, name, now(), author],
        )?;
        tx.commit()?;
        self.table(&key)
    }

    /// Edit a table's identity — its display name, its key, or both.
    ///
    /// The two are a single act because they are a single form: the person is
    /// deciding what this table is called, and the key is the half of that
    /// answer SQL sees. Renaming alone leaves the key exactly where it is,
    /// which is the point of the split; moving the key is a separate,
    /// deliberate thing that breaks the queries already written against it.
    ///
    /// The name upserts, so a table that reached the file without a name row
    /// (made outside these tools) can still be given one.
    pub fn update(&mut self, table: &str, name: Option<&str>, key: Option<&str>) -> Result<Table> {
        let mut current = self.resolve(table)?;

        if let Some(wanted) = key.map(str::trim).filter(|key| !key.is_empty()) {
            let next = slug(wanted);
            if next != current {
                if self.exists(&next)? {
                    bail!("{next} is taken");
                }
                let tx = self.writer.transaction()?;
                tx.execute_batch(&format!(
                    "ALTER TABLE {} RENAME TO {}",
                    quote(&current),
                    quote(&next)
                ))?;
                tx.execute(
                    "UPDATE _tables SET key = ?2 WHERE key = ?1",
                    [&current, &next],
                )?;
                tx.commit()?;
                current = next;
            }
        }

        if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
            self.writer.execute(
                "INSERT INTO _tables (key, name, created_at, author) VALUES (?1, ?2, ?3, NULL) \
                 ON CONFLICT(key) DO UPDATE SET name = ?2",
                rusqlite::params![&current, name, now()],
            )?;
        }
        self.table(&current)
    }

    /// Take a table away — its rows and its name together.
    ///
    /// Not reachable from a tool, and that is deliberate: an agent fills a
    /// table in, the person decides whether it goes on existing.
    pub fn remove(&mut self, table: &str) -> Result<()> {
        let key = self.resolve(table)?;
        let tx = self.writer.transaction()?;
        tx.execute_batch(&format!("DROP TABLE {}", quote(&key)))?;
        tx.execute("DELETE FROM _tables WHERE key = ?1", [&key])?;
        Ok(tx.commit()?)
    }

    /// A table by its key or its display name, answering with the key. Both are
    /// accepted because the key is what a query carries and the name is what a
    /// person says.
    fn resolve(&self, ident: &str) -> Result<String> {
        let ident = ident.trim();
        self.reader
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1 \
                 UNION ALL SELECT key FROM _tables WHERE name = ?1 COLLATE NOCASE",
                [ident],
                |row| row.get(0),
            )
            .map_err(|_| anyhow!("no table {ident}"))
    }

    fn exists(&self, key: &str) -> Result<bool> {
        Ok(self
            .reader
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [key],
                |row| row.get::<_, i64>(0),
            )
            .is_ok())
    }

    /// A free identifier derived from the name, suffixed until it is unused.
    /// Taken from `sqlite_master` rather than `_tables`, so a key can never
    /// collide with a table that exists but was never registered.
    fn mint_key(&self, name: &str) -> Result<String> {
        let base = slug(name);
        if !self.exists(&base)? {
            return Ok(base);
        }
        for n in 2.. {
            let key = format!("{base}_{n}");
            if !self.exists(&key)? {
                return Ok(key);
            }
        }
        unreachable!("an unbounded range holds a free suffix")
    }
}

/// The columns of one table, straight out of the catalog: name and declared
/// type, in the order the table declares them.
pub(crate) fn columns_of(conn: &Connection, key: &str) -> Result<Vec<Column>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(key)))?;
    let columns = stmt
        .query_map([], |row| {
            Ok(Column {
                name: row.get("name")?,
                kind: ColType::from(row.get::<_, String>("type").unwrap_or_default().as_str()),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}

/// One cell as JSON, by what SQLite actually stored rather than by what the
/// column was declared — a `NUMERIC` column holds whichever of the two the
/// value converted to, and the reader wants the value.
pub(crate) fn value(row: &Row, i: usize) -> Value {
    match row.get_ref(i) {
        Ok(ValueRef::Integer(n)) => Value::from(n),
        Ok(ValueRef::Real(f)) => Value::from(f),
        Ok(ValueRef::Text(t)) => Value::from(String::from_utf8_lossy(t).into_owned()),
        // Nothing here writes a blob, and there is no JSON form of one that a
        // reader would not have to guess at.
        _ => Value::Null,
    }
}

/// Bind a JSON value as whatever SQLite should store. The column's affinity
/// converts from here, so a number typed into a text column and a date typed
/// into a number one both land the way the declaration says.
pub(crate) fn bind(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match v {
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map_or_else(|| n.as_f64().map_or(Sql::Null, Sql::Real), Sql::Integer),
        Value::String(s) => Sql::Text(s.clone()),
        // Null, and anything with no cell form (an array, an object) — a cell
        // the person emptied and a value we cannot store read the same.
        _ => Sql::Null,
    }
}

/// An identifier as SQL will accept it whatever it contains, which is what lets
/// a column keep the header the person typed.
pub(crate) fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// A SQL identifier from a display name. Lossy on purpose — it is a handle, not
/// a translation, and a name that reduces to nothing (any name in a non-Latin
/// script) still gets a usable one, because the display name is where the
/// meaning lives.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_');
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("t_{out}")
    } else {
        out.to_owned()
    }
}

/// Seconds since the epoch, as SQLite stores an integer.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}
