//! Shape changes. One entry point, because add, rename and retype differ only
//! by which arguments arrive — and because the shape is the thing the display
//! reads back, so every path to it has to be the same path.

use crate::data::{self, ColType, Column, Data};
use anyhow::{Result, bail};

impl Data {
    /// Add a column, rename one, or change its type.
    ///
    /// `column` names the column to act on. It does not exist yet → it is
    /// added, and `kind` says what it holds. It does exist → `rename` moves its
    /// name and `kind` changes its type; either, or both.
    pub fn write_column(
        &mut self,
        table: &str,
        column: &str,
        kind: Option<ColType>,
        rename: Option<&str>,
    ) -> Result<Vec<Column>> {
        let key = self.resolve(table)?;
        let column = column.trim();
        if column.is_empty() {
            bail!("a column needs a name");
        }
        let existing = data::columns_of(&self.writer, &key)?;
        let at = existing
            .iter()
            .position(|col| col.name.eq_ignore_ascii_case(column));

        let Some(at) = at else {
            let Some(kind) = kind else {
                bail!("no column {column} on {key} — say what it holds to add it");
            };
            if rename.is_some() {
                bail!("no column {column} on {key} to rename");
            }
            self.writer.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN {} {}",
                data::quote(&key),
                data::quote(column),
                kind.sql()
            ))?;
            return data::columns_of(&self.writer, &key);
        };

        if let Some(new) = rename.map(str::trim) {
            if new.is_empty() {
                bail!("a column needs a name");
            }
            if existing
                .iter()
                .enumerate()
                .any(|(i, col)| i != at && col.name.eq_ignore_ascii_case(new))
            {
                bail!("{key} already has a column {new}");
            }
            self.writer.execute_batch(&format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                data::quote(&key),
                data::quote(&existing[at].name),
                data::quote(new)
            ))?;
        }

        if let Some(kind) = kind.filter(|kind| *kind != existing[at].kind) {
            let mut next = data::columns_of(&self.writer, &key)?;
            next[at].kind = kind;
            self.rebuild(&key, &next)?;
        }
        data::columns_of(&self.writer, &key)
    }

    /// Put the columns in this order.
    ///
    /// Column order is the one ordering in a table that is real and not a
    /// query: SQL has no `ORDER BY` for it and no statement that changes it, so
    /// it costs the same rebuild a retype does. Row order gets no such method
    /// on purpose — rows are ordered by what you sort them on, and giving them
    /// a stored position would mean a column in the person's data they never
    /// asked for and every agent insert would have to maintain.
    ///
    /// `order` must name exactly the columns the table has. A caller that
    /// dropped one would otherwise rebuild the table without it, which is a
    /// column of data deleted by a drag.
    pub fn reorder_columns(&mut self, table: &str, order: &[String]) -> Result<Vec<Column>> {
        let key = self.resolve(table)?;
        let existing = data::columns_of(&self.writer, &key)?;
        if order.len() != existing.len()
            || !order
                .iter()
                .all(|name| existing.iter().any(|col| &col.name == name))
        {
            bail!("that ordering is not this table's columns");
        }
        let next: Vec<Column> = order
            .iter()
            .filter_map(|name| existing.iter().find(|col| &col.name == name).cloned())
            .collect();
        if next
            .iter()
            .map(|col| &col.name)
            .eq(existing.iter().map(|col| &col.name))
        {
            return Ok(existing);
        }
        self.rebuild(&key, &next)?;
        data::columns_of(&self.writer, &key)
    }

    /// Retype by rebuilding the table around the new declaration, which is the
    /// only way SQLite changes a column's type.
    ///
    /// The copy needs no `CAST`: affinity is applied on insert, so each value
    /// converts if it can and stays as it is if it cannot — which is the honest
    /// outcome for a column of hand-typed text becoming a date.
    ///
    /// Safe to `DROP` mid-transaction because nothing in this file carries a
    /// foreign key: table-to-table relations are deliberately unbuilt, so there
    /// are no children for the drop to cascade into.
    fn rebuild(&mut self, key: &str, columns: &[Column]) -> Result<()> {
        let tmp = format!("{key}_rebuilding");
        let defs = columns
            .iter()
            .map(|col| format!("{} {}", data::quote(&col.name), col.kind.sql()))
            .collect::<Vec<_>>()
            .join(", ");
        let names = columns
            .iter()
            .map(|col| data::quote(&col.name))
            .collect::<Vec<_>>()
            .join(", ");

        let tx = self.writer.transaction()?;
        for stmt in [
            format!("DROP TABLE IF EXISTS {}", data::quote(&tmp)),
            format!("CREATE TABLE {} ({defs})", data::quote(&tmp)),
            format!(
                "INSERT INTO {} ({names}) SELECT {names} FROM {}",
                data::quote(&tmp),
                data::quote(key)
            ),
            format!("DROP TABLE {}", data::quote(key)),
            format!(
                "ALTER TABLE {} RENAME TO {}",
                data::quote(&tmp),
                data::quote(key)
            ),
        ] {
            tx.execute_batch(&stmt)?;
        }
        Ok(tx.commit()?)
    }
}
