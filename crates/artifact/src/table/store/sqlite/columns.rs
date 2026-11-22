use crate::table::{
    self, ColType, ColumnDisplay, ColumnFormat, ColumnLayout,
    store::sqlite::{self as data, Data},
};
use anyhow::{Result, bail};
use rusqlite::{Connection, TransactionBehavior, params};

pub(crate) fn layout(db: &Connection, key: &str) -> Result<ColumnLayout> {
    let mut query =
        db.prepare("SELECT column_name, display FROM _cydonia_table_columns WHERE table_key = ?1")?;
    let rows = query.query_map([key], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let settings = rows
        .map(|row| {
            let (name, settings) = row?;
            Ok((name, serde_json::from_str(&settings)?))
        })
        .collect::<Result<_>>()?;
    Ok(ColumnLayout { settings })
}

pub(crate) fn rename_display(db: &Connection, key: &str, before: &str, after: &str) -> Result<()> {
    db.execute(
        "UPDATE _cydonia_table_columns SET column_name = ?3 WHERE table_key = ?1 AND column_name = ?2",
        params![key, before, after],
    )?;
    Ok(())
}

pub(super) fn seed_select_options(db: &Connection, key: &str, column: &str) -> Result<()> {
    let mut display = layout(db, key)?.settings.remove(column).unwrap_or_default();
    let field = table::quote(column);
    let mut query = db.prepare(&format!(
        "SELECT DISTINCT {field} COLLATE BINARY FROM {} WHERE {field} IS NOT NULL AND {field} != '' ORDER BY 1",
        table::quote(key),
    ))?;
    for value in query.query_map([], |row| row.get::<_, String>(0))? {
        let value = value?;
        if !display.select_options.contains(&value) {
            display.select_options.push(value);
        }
    }
    write_display(db, key, column, &display)
}

impl Data {
    pub fn write_option(
        &mut self,
        table: &str,
        column: &str,
        previous: Option<&str>,
        name: &str,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!("an option needs a name");
        }
        let key = self.resolve(table)?;
        let tx = self
            .writer
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !data::columns_of(&tx, &key)?
            .iter()
            .any(|held| held.name == column && held.kind == ColType::Select)
        {
            bail!("no select column {column} on {key}");
        }
        let mut display = layout(&tx, &key)?
            .settings
            .remove(column)
            .unwrap_or_default();
        if previous == Some(name) {
            return Ok(());
        }
        if display.select_options.iter().any(|held| held == name) {
            bail!("the option {name} already exists");
        }
        if let Some(previous) = previous {
            let Some(held) = display
                .select_options
                .iter_mut()
                .find(|held| held.as_str() == previous)
            else {
                bail!("the option {previous} no longer exists");
            };
            *held = name.to_owned();
            let field = table::quote(column);
            tx.execute(
                &format!(
                    "UPDATE {} SET {field} = ?1 WHERE {field} = ?2 COLLATE BINARY",
                    table::quote(&key)
                ),
                [name, previous],
            )?;
        } else {
            display.select_options.push(name.to_owned());
        }
        write_display(&tx, &key, column, &display)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_column_width(&mut self, table: &str, column: &str, width: u32) -> Result<()> {
        self.edit_column_display(table, column, |_, display| {
            display.width = Some(width);
            Ok(())
        })
    }

    pub fn set_column_format(
        &mut self,
        table: &str,
        column: &str,
        format: ColumnFormat,
    ) -> Result<()> {
        self.edit_column_display(table, column, |kind, display| format.apply(kind, display))
    }

    fn edit_column_display(
        &mut self,
        table: &str,
        column: &str,
        edit: impl FnOnce(ColType, &mut ColumnDisplay) -> Result<()>,
    ) -> Result<()> {
        let key = self.resolve(table)?;
        let tx = self
            .writer
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(kind) = data::columns_of(&tx, &key)?
            .into_iter()
            .find(|held| held.name == column)
            .map(|column| column.kind)
        else {
            bail!("no column {column} on {key}");
        };
        let mut settings = layout(&tx, &key)?
            .settings
            .remove(column)
            .unwrap_or_default();
        edit(kind, &mut settings)?;
        write_display(&tx, &key, column, &settings)?;
        tx.commit()?;
        Ok(())
    }

    pub fn move_column(
        &mut self,
        table: &str,
        column: &str,
        after: Option<&str>,
        before: Option<&str>,
    ) -> Result<()> {
        let key = self.resolve(table)?;
        let tx = self
            .writer
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut layout = layout(&tx, &key)?;
        let columns = data::columns_of(&tx, &key)?;
        let mut names: Vec<_> = layout
            .order(&columns)
            .into_iter()
            .map(|ix| columns[ix].name.as_str())
            .collect();
        let Some(index) = names.iter().position(|name| *name == column) else {
            bail!("no column {column} on {key}");
        };
        let moving = names.remove(index);
        let target = before
            .and_then(|name| names.iter().position(|held| *held == name))
            .or_else(|| {
                after.and_then(|name| names.iter().position(|held| *held == name).map(|ix| ix + 1))
            });
        let Some(target) = target else {
            return Ok(());
        };
        names.insert(target, moving);
        for (position, name) in names.into_iter().enumerate() {
            let display = layout.settings.entry(name.to_owned()).or_default();
            display.position = Some(position);
            write_display(&tx, &key, name, display)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn write_display(db: &Connection, key: &str, name: &str, display: &ColumnDisplay) -> Result<()> {
    db.execute(
        "INSERT INTO _cydonia_table_columns (table_key, column_name, display) VALUES (?1, ?2, ?3)
         ON CONFLICT(table_key, column_name) DO UPDATE SET display = excluded.display",
        params![key, name, serde_json::to_string(display)?],
    )?;
    Ok(())
}
