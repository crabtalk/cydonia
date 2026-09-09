//! Rows and cells — what a reader reads and a writer edits.
//!
//! Everything here addresses a row by its `rowid`, SQLite's own identity for
//! it: a handle that survives a sort, an insert above it and a neighbour's
//! deletion, with no surrogate key column to hide from the person and no
//! position index that means something different the moment the order changes.

use crate::data::{self, Data, Edit, Page, Row};
use anyhow::{Result, bail};
use rusqlite::params_from_iter;
use schema::table::Column;

impl Data {
    /// Read a window of rows.
    ///
    /// `sort` names a column to order by, and is checked against the table's
    /// real columns before it reaches the statement — the one place a caller's
    /// string would otherwise be spliced into SQL unquoted.
    pub fn read(
        &self,
        table: &str,
        sort: Option<&str>,
        desc: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Page> {
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.reader, &key)?;
        let name = self
            .reader
            .query_row("SELECT name FROM _tables WHERE key = ?1", [&key], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| key.clone());

        let selected = columns
            .iter()
            .map(|col| data::quote(&col.name))
            .collect::<Vec<_>>()
            .join(", ");
        let projection = match columns.is_empty() {
            true => "rowid".to_owned(),
            false => format!("rowid, {selected}"),
        };
        // Empty sorts with null: a blank cell is absent, whichever direction
        // you are reading in, and burying it under the values is what makes a
        // column of a few filled rows readable at all.
        let order = match sort.and_then(|s| columns.iter().find(|col| col.name == s)) {
            Some(col) => format!(
                " ORDER BY ({0} IS NULL OR {0} = '') , {0} {1}",
                data::quote(&col.name),
                if desc { "DESC" } else { "ASC" }
            ),
            None => String::new(),
        };

        let count = columns.len();
        let mut stmt = self.reader.prepare(&format!(
            "SELECT {projection} FROM {}{order} LIMIT ?1 OFFSET ?2",
            data::quote(&key)
        ))?;
        let rows = stmt
            .query_map([limit, offset], |row| {
                Ok(Row {
                    rowid: row.get(0)?,
                    cells: (1..=count).map(|i| data::value(row, i)).collect(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Page {
            total: self.reader.query_row(
                &format!("SELECT count(*) FROM {}", data::quote(&key)),
                [],
                |row| row.get(0),
            )?,
            rows,
            name,
            key,
            columns,
            offset,
        })
    }

    /// Write cells. One transaction, so a pasted block lands whole or not at
    /// all — a half-applied paste is worse than a rejected one, because there
    /// is nothing on screen saying which half.
    pub fn write_cells(&mut self, table: &str, edits: &[Edit]) -> Result<()> {
        if edits.is_empty() {
            return Ok(());
        }
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        for edit in edits {
            if !columns.iter().any(|col| col.name == edit.column) {
                bail!("no column {} on {key}", edit.column);
            }
        }
        let tx = self.writer.transaction()?;
        for edit in edits {
            tx.execute(
                &format!(
                    "UPDATE {} SET {} = ?1 WHERE rowid = ?2",
                    data::quote(&key),
                    data::quote(&edit.column)
                ),
                rusqlite::params![data::bind(&edit.value), edit.rowid],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// Append empty rows, answering with their ids in the order they were made.
    pub fn add_rows(&mut self, table: &str, count: i64) -> Result<Vec<i64>> {
        let key = self.resolve(table)?;
        let sql = format!("INSERT INTO {} DEFAULT VALUES", data::quote(&key));
        let tx = self.writer.transaction()?;
        let mut ids = Vec::new();
        for _ in 0..count.max(0) {
            tx.execute(&sql, [])?;
            ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn delete_rows(&mut self, table: &str, rowids: &[i64]) -> Result<()> {
        if rowids.is_empty() {
            return Ok(());
        }
        let key = self.resolve(table)?;
        let places = vec!["?"; rowids.len()].join(", ");
        self.writer.execute(
            &format!(
                "DELETE FROM {} WHERE rowid IN ({places})",
                data::quote(&key)
            ),
            params_from_iter(rowids),
        )?;
        Ok(())
    }

    /// Take a column away. Not reachable from a tool — the shape is the
    /// person's, same line the table itself sits on.
    pub fn drop_column(&mut self, table: &str, column: &str) -> Result<Vec<Column>> {
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        let Some(col) = columns.iter().find(|col| col.name == column) else {
            bail!("no column {column} on {key}");
        };
        if columns.len() == 1 {
            bail!("a table keeps at least one column");
        }
        self.writer.execute_batch(&format!(
            "ALTER TABLE {} DROP COLUMN {}",
            data::quote(&key),
            data::quote(&col.name)
        ))?;
        data::columns_of(&self.writer, &key)
    }
}
