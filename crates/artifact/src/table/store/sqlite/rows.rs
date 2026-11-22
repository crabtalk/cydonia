use crate::table::{
    self, Column,
    rows::{Edit, Page, Revision, Row},
    store::sqlite::{self as data, Data, identity::Identity, row_order},
};
use anyhow::{Result, bail};
use rusqlite::{ToSql, Transaction, TransactionBehavior, params_from_iter};
use serde_json::{Map, Value};

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
        let guard = Transaction::new_unchecked(&self.writer, TransactionBehavior::Immediate)?;
        let revision = self.revision()?;
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.reader, &key)?;
        let rowid = Identity::for_table(&self.reader, &key, None)?.0;
        let name = self
            .reader
            .query_row("SELECT name FROM _tables WHERE key = ?1", [&key], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| key.clone());

        let selected = columns
            .iter()
            .map(|col| table::quote(&col.name))
            .collect::<Vec<_>>()
            .join(", ");
        let projection = match columns.is_empty() {
            true => rowid.to_owned(),
            false => format!("{rowid}, {selected}"),
        };
        // Empty sorts with null: a blank cell is absent, whichever direction
        // you are reading in, and burying it under the values is what makes a
        // column of a few filled rows readable at all.
        let sorted = sort.and_then(|s| columns.iter().find(|col| col.name == s));
        let manual = sorted.is_none() && row_order::ordered(&self.reader, &key)?;
        let order = match sorted {
            Some(col) => {
                let column = table::quote(&col.name);
                let direction = if desc { "DESC" } else { "ASC" };
                let blank = format!("{column} IS NULL OR {column} = ''");
                format!(
                    " ORDER BY ({blank}), CASE WHEN {blank} THEN {rowid} END, {column} {direction}, {rowid}"
                )
            }
            None if manual => row_order::order_by(rowid, "?3"),
            None => format!(" ORDER BY {rowid}"),
        };

        let count = columns.len();
        let mut stmt = self.reader.prepare(&format!(
            "SELECT {projection} FROM {} AS _rows{order} LIMIT ?1 OFFSET ?2",
            table::quote(&key)
        ))?;
        let bindings: [&dyn ToSql; 3] = [&limit, &offset, &key];
        let rows = stmt
            .query_map(&bindings[..if manual { 3 } else { 2 }], |row| {
                Ok(Row {
                    rowid: row.get(0)?,
                    cells: (1..=count).map(|i| data::value(row, i)).collect(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        guard.commit()?;
        Ok(Page {
            revision,
            total: self.reader.query_row(
                &format!("SELECT count(*) FROM {}", table::quote(&key)),
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

    pub(super) fn revision(&self) -> Result<Revision> {
        Ok(Revision {
            connection: self.identity,
            changes: self.writer.total_changes(),
            external: self
                .writer
                .pragma_query_value(None, "data_version", |row| row.get(0))?,
            schema: self
                .writer
                .pragma_query_value(None, "schema_version", |row| row.get(0))?,
        })
    }

    pub fn write_cells(&mut self, table: &str, edits: &[Edit]) -> Result<()> {
        self.write_cells_at(table, edits, None)
    }

    /// A revision mismatch rejects the whole batch without writing any cells.
    pub fn write_cells_at(
        &mut self,
        table: &str,
        edits: &[Edit],
        revision: Option<Revision>,
    ) -> Result<()> {
        if edits.is_empty() {
            return Ok(());
        }
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        let rowid = Identity::for_table(&self.writer, &key, None)?.0;
        for edit in edits {
            if !columns.iter().any(|col| col.name == edit.column) {
                bail!("no column {} on {key}", edit.column);
            }
        }
        let tx = Transaction::new_unchecked(&self.writer, TransactionBehavior::Immediate)?;
        if let Some(revision) = revision
            && revision != self.revision()?
        {
            bail!(
                "table data changed while editing; copy your draft, press Escape, and reopen the cell to retry"
            );
        }
        for edit in edits {
            let changed = tx.execute(
                &format!(
                    "UPDATE {} SET {} = ?1 WHERE {rowid} = ?2",
                    table::quote(&key),
                    table::quote(&edit.column)
                ),
                rusqlite::params![data::bind(&edit.value), edit.rowid],
            )?;
            if changed != 1 {
                bail!("no row {} on {key}", edit.rowid);
            }
        }
        Ok(tx.commit()?)
    }

    /// Append empty rows, answering with their ids in the order they were made.
    pub fn add_rows(&mut self, table: &str, count: i64) -> Result<Vec<i64>> {
        let key = self.resolve(table)?;
        Identity::for_table(&self.writer, &key, None)?;
        let sql = format!("INSERT INTO {} DEFAULT VALUES", table::quote(&key));
        let tx = self.writer.transaction()?;
        let mut ids = Vec::new();
        for _ in 0..count.max(0) {
            tx.execute(&sql, [])?;
            ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Append rows, each by column name, in one transaction, answering with
    /// their ids in order. A column the table does not have fails the lot.
    pub fn insert(&mut self, table: &str, rows: &[Map<String, Value>]) -> Result<Vec<i64>> {
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        Identity::for_table(&self.writer, &key, None)?;
        for row in rows {
            if let Some(name) = row
                .keys()
                .find(|name| !columns.iter().any(|col| &col.name == *name))
            {
                bail!("no column {name} on {key}");
            }
        }
        let tx = self.writer.transaction()?;
        let mut ids = Vec::with_capacity(rows.len());
        for row in rows {
            match row.is_empty() {
                true => tx.execute(
                    &format!("INSERT INTO {} DEFAULT VALUES", table::quote(&key)),
                    [],
                )?,
                false => tx.execute(
                    &format!(
                        "INSERT INTO {} ({}) VALUES ({})",
                        table::quote(&key),
                        row.keys()
                            .map(|name| table::quote(name))
                            .collect::<Vec<_>>()
                            .join(", "),
                        vec!["?"; row.len()].join(", ")
                    ),
                    params_from_iter(row.values().map(data::bind)),
                )?,
            };
            ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Take away the rows whose `column` holds `value`.
    pub fn delete_where(&mut self, table: &str, column: &str, value: &Value) -> Result<()> {
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        let Some(col) = columns.iter().find(|col| col.name == column) else {
            bail!("no column {column} on {key}");
        };
        self.writer.execute(
            &format!(
                "DELETE FROM {} WHERE {} IS ?1",
                table::quote(&key),
                table::quote(&col.name)
            ),
            [data::bind(value)],
        )?;
        Ok(())
    }

    pub fn delete_rows(&mut self, table: &str, rowids: &[i64]) -> Result<()> {
        if rowids.is_empty() {
            return Ok(());
        }
        let key = self.resolve(table)?;
        let rowid = Identity::for_table(&self.writer, &key, None)?.0;
        let places = vec!["?"; rowids.len()].join(", ");
        self.writer.execute(
            &format!(
                "DELETE FROM {} WHERE {rowid} IN ({places})",
                table::quote(&key)
            ),
            params_from_iter(rowids),
        )?;
        Ok(())
    }

    pub fn drop_column(&mut self, table: &str, column: &str) -> Result<Vec<Column>> {
        let key = self.resolve(table)?;
        let columns = data::columns_of(&self.writer, &key)?;
        let Some(col) = columns.iter().find(|col| col.name == column) else {
            bail!("no column {column} on {key}");
        };
        if columns.len() == 1 {
            bail!("a table keeps at least one column");
        }
        let tx = self.writer.transaction()?;
        tx.execute_batch(&format!(
            "ALTER TABLE {} DROP COLUMN {}",
            table::quote(&key),
            table::quote(&col.name)
        ))?;
        tx.execute(
            "DELETE FROM _cydonia_table_columns WHERE table_key = ?1 AND column_name = ?2",
            rusqlite::params![key, col.name],
        )?;
        tx.commit()?;
        data::columns_of(&self.writer, &key)
    }
}
