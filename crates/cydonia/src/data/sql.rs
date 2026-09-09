//! Raw SQL — the half of the surface that is deliberately not typed.
//!
//! DDL goes through tools so the shape the display reads back is always the
//! shape that was written; DML stays raw, because that is what makes a batch
//! write one call instead of two hundred.

use crate::data::{self, Data, Rows};
use anyhow::{Result, bail};

/// What a write may begin with. Not a filter over what SQL can express — a
/// list of what this surface offers. Anything else is a shape change, and
/// shape changes have their own entry point.
const WRITES: [&str; 3] = ["insert", "update", "delete"];

impl Data {
    /// Run a `SELECT`.
    ///
    /// On the read-only handle, so the guarantee is the connection rather than
    /// anything believed about the text. The single-statement rule is here only
    /// so a caller who sent two gets told which answer they lost.
    pub fn query(&self, sql: &str) -> Result<Rows> {
        let mut stmts = split(sql);
        let Some(stmt) = stmts.pop() else {
            bail!("empty statement");
        };
        if !stmts.is_empty() {
            bail!("one statement at a time");
        }
        let mut stmt = self.reader.prepare(&stmt)?;
        let columns: Vec<String> = stmt.column_names().into_iter().map(str::to_owned).collect();
        let count = columns.len();
        let rows = stmt
            .query_map([], |row| {
                Ok((0..count).map(|i| data::value(row, i)).collect())
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Rows { columns, rows })
    }

    /// Run inserts, updates and deletes, answering with the rows touched.
    ///
    /// Each statement is checked and run on its own rather than handed over as
    /// one string: a leading-keyword check on the whole text says nothing about
    /// what follows the first semicolon, which is the difference between a
    /// guard and the appearance of one.
    pub fn write(&mut self, sql: &str) -> Result<u64> {
        let stmts = split(sql);
        if stmts.is_empty() {
            bail!("empty statement");
        }
        for stmt in &stmts {
            let verb = stmt
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !WRITES.contains(&verb.as_str()) {
                bail!(
                    "`{verb}` is not one of {} — a shape change goes through `write_column`",
                    WRITES.join(", ")
                );
            }
        }
        let tx = self.writer.transaction()?;
        let mut touched = 0;
        for stmt in &stmts {
            touched += tx.execute(stmt, [])? as u64;
        }
        tx.commit()?;
        Ok(touched)
    }
}

/// Split on the semicolons that separate statements, skipping the ones inside
/// string literals, quoted identifiers and comments — where a semicolon is just
/// a character in the person's data.
fn split(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stmt = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' | '"' | '`' => {
                stmt.push(ch);
                // A doubled quote inside a literal is an escaped one, and the
                // second half re-opens it as far as this loop is concerned —
                // which lands on the same place either way.
                for q in chars.by_ref() {
                    stmt.push(q);
                    if q == ch {
                        break;
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            ';' => out.push(std::mem::take(&mut stmt)),
            _ => stmt.push(ch),
        }
    }
    out.push(stmt);
    out.into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}
