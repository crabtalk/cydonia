//! What the agents' logs have spent in a project, indexed in its `state.db`:
//! a row per model request, and how far each log has been read. Logs only
//! grow, so a refresh reads what was appended since; a log shorter than its
//! offset was rewritten and is read again. Everything here can be rebuilt
//! from the logs that still exist.

use super::{Logs, Request, Tokens, claude, codex, local_day, settled};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension as _, params};
use std::{
    io::{BufRead as _, Seek as _},
    path::{Path, PathBuf},
    time::Duration,
};

const DDL: &str = "
CREATE TABLE IF NOT EXISTS agent_requests (
    id TEXT PRIMARY KEY,
    day TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS agent_requests_day ON agent_requests (day);
CREATE TABLE IF NOT EXISTS agent_logs (
    path TEXT PRIMARY KEY,
    offset INTEGER NOT NULL,
    state TEXT NOT NULL
);
";

pub(super) struct Index {
    connection: Connection,
    project: PathBuf,
}

impl Index {
    pub(super) fn open(project: &Path) -> Result<Self> {
        let path = artifact::project::fs::Project::new(project).state()?;
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(DDL)?;
        Ok(Self {
            connection,
            project: settled(project),
        })
    }

    /// Read what every log has had appended since it was last read.
    pub(super) fn refresh(&mut self, logs: &Logs) -> Result<()> {
        let tx = self.connection.transaction()?;
        if let Some(dir) = &logs.claude {
            let folder = dir.join(claude::folder_name(&self.project));
            for (file, len) in logs_under(&folder) {
                read(&tx, &file, len, |state, line, _| {
                    (state.to_owned(), claude::parse(line))
                })?;
            }
        }
        if let Some(dir) = &logs.codex {
            for (file, len) in logs_under(dir) {
                let key = file.to_string_lossy().into_owned();
                read(&tx, &file, len, |state, line, at| {
                    let mut thread = codex::Thread::restore(state);
                    let request = thread.step(line, &self.project);
                    (
                        thread.save(),
                        request.map(|request| (format!("{key}:{at}"), request)),
                    )
                })?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Tokens by day and model, from `from` on.
    pub(super) fn spend(&self, from: &str) -> Result<Vec<(String, String, Tokens)>> {
        let mut statement = self.connection.prepare(
            "SELECT day, model, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write)
             FROM agent_requests WHERE day >= ?1 GROUP BY day, model",
        )?;
        let rows = statement.query_map([from], |row| {
            let count = |ix: usize| row.get::<_, i64>(ix).map(|n| n.max(0) as u64);
            Ok((
                row.get(0)?,
                row.get(1)?,
                Tokens {
                    input: count(2)?,
                    output: count(3)?,
                    cache_read: count(4)?,
                    cache_write: count(5)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Read `file` from where it was left, `step` turning each whole line — with
/// the log's state and the line's byte offset — into the state after it and
/// the request it records, if any. A line still being written is left for the
/// next read.
fn read(
    tx: &rusqlite::Transaction,
    file: &Path,
    len: u64,
    mut step: impl FnMut(&str, &str, u64) -> (String, Option<(String, Request)>),
) -> Result<()> {
    let key = file.to_string_lossy();
    let known: Option<(i64, String)> = tx
        .query_row(
            "SELECT offset, state FROM agent_logs WHERE path = ?1",
            [&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (mut offset, mut state) = match known {
        Some((offset, state)) if offset as u64 <= len => (offset as u64, state),
        _ => (0, String::new()),
    };
    if offset == len {
        return Ok(());
    }
    let mut reader = std::io::BufReader::new(std::fs::File::open(file)?);
    reader.seek(std::io::SeekFrom::Start(offset))?;
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 || !line.ends_with('\n') {
            break;
        }
        let at = offset;
        offset += read as u64;
        let (after, request) = step(&state, line.trim_end(), at);
        state = after;
        if let Some((id, request)) = request {
            let Some(day) = local_day(request.at.timestamp()) else {
                continue;
            };
            let t = request.tokens;
            tx.execute(
                "INSERT OR IGNORE INTO agent_requests
                 (id, day, model, input, output, cache_read, cache_write)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    day,
                    request.model,
                    t.input as i64,
                    t.output as i64,
                    t.cache_read as i64,
                    t.cache_write as i64
                ],
            )?;
        }
    }
    tx.execute(
        "INSERT INTO agent_logs (path, offset, state) VALUES (?1, ?2, ?3)
         ON CONFLICT (path) DO UPDATE SET offset = ?2, state = ?3",
        params![key, offset as i64, state],
    )?;
    Ok(())
}

/// Every `.jsonl` file under `dir`, with its length.
fn logs_under(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                found.push((path, meta.len()));
            }
        }
    }
    found
}
