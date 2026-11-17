use crate::table::{
    self, ColType, Column,
    store::sqlite::{self as data, Data, columns, identity::Identity, schema::Schema},
};
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

        Identity::for_table(
            &self.writer,
            &key,
            Some((column, rename.map(str::trim).unwrap_or(column))),
        )?;
        let Some(at) = at else {
            let Some(kind) = kind else {
                bail!("no column {column} on {key} — say what it holds to add it");
            };
            if rename.is_some() {
                bail!("no column {column} on {key} to rename");
            }
            self.writer.execute_batch(&format!(
                "ALTER TABLE {} ADD COLUMN {} {}",
                table::quote(&key),
                table::quote(column),
                kind.sql()
            ))?;
            return data::columns_of(&self.writer, &key);
        };

        if let Some(kind) = kind.filter(|kind| *kind != existing[at].kind) {
            self.rebuild(&key, &existing[at].name, kind, rename)?;
            return data::columns_of(&self.writer, &key);
        }
        let tx = self.writer.savepoint()?;
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
            tx.execute_batch(&format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                table::quote(&key),
                table::quote(&existing[at].name),
                table::quote(new)
            ))?;
            columns::rename_display(&tx, &key, &existing[at].name, new)?;
        }
        tx.commit()?;
        data::columns_of(&self.writer, &key)
    }

    fn rebuild(
        &mut self,
        key: &str,
        column: &str,
        kind: ColType,
        rename: Option<&str>,
    ) -> Result<()> {
        let tmp = self.mint_key(&format!("{key}_rebuilding"))?;
        let schema = Schema(self.writer.query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [key],
            |row| row.get(0),
        )?);
        let create = schema.retype(&tmp, column, kind)?;
        let rowid = Identity::for_table(&self.writer, key, None)?.0;
        let primary: bool = self.writer.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2 AND pk <> 0)",
            [key, column],
            |row| row.get(0),
        )?;
        if primary {
            bail!("a primary key column cannot be retyped");
        }
        let objects = self.writer.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name = ?1 AND type IN ('index', 'trigger') AND sql IS NOT NULL")?
            .query_map([key], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let names = data::columns_of(&self.writer, key)?
            .iter()
            .map(|col| table::quote(&col.name))
            .collect::<Vec<_>>()
            .join(", ");
        let foreign_keys: bool = self
            .writer
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        self.writer.pragma_update(None, "foreign_keys", false)?;
        let result = (|| -> Result<()> {
            let tx = self.writer.transaction()?;
            tx.execute_batch(&create)?;
            tx.execute_batch(&format!(
                "INSERT INTO {} ({rowid}, {names}) SELECT {rowid}, {names} FROM {}",
                table::quote(&tmp),
                table::quote(key)
            ))?;
            tx.execute_batch(&format!("DROP TABLE {}", table::quote(key)))?;
            tx.execute_batch(&format!(
                "ALTER TABLE {} RENAME TO {}",
                table::quote(&tmp),
                table::quote(key)
            ))?;
            for sql in objects {
                tx.execute_batch(&sql)?;
            }
            if let Some(new) = rename {
                if new.trim().is_empty() {
                    bail!("a column needs a name");
                }
                tx.execute_batch(&format!(
                    "ALTER TABLE {} RENAME COLUMN {} TO {}",
                    table::quote(key),
                    table::quote(column),
                    table::quote(new.trim())
                ))?;
                columns::rename_display(&tx, key, column, new.trim())?;
            }
            let violations: i64 =
                tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
            if violations != 0 {
                bail!("column type change violates foreign keys");
            }
            tx.commit()?;
            Ok(())
        })();
        self.writer
            .pragma_update(None, "foreign_keys", foreign_keys)?;
        result
    }
}
