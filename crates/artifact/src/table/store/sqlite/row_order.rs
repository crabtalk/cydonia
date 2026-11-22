use crate::table::{
    self,
    rows::Revision,
    store::sqlite::{Data, identity::Identity},
};
use anyhow::{Result, bail};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

const EVENTS: [(&str, &str); 3] = [
    ("insert", "INSERT"),
    ("delete", "DELETE"),
    ("update", "UPDATE"),
];

pub(super) fn ordered(db: &Connection, key: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS (SELECT 1 FROM _cydonia_table_row_order WHERE table_key = ?1)",
        [key],
        |row| row.get(0),
    )?)
}

pub(super) fn order_by(row_id: &str, key: &str) -> String {
    let rank = format!(
        "(SELECT position FROM _cydonia_table_row_order WHERE table_key = {key} AND row_id = _rows.{row_id})"
    );
    format!(" ORDER BY {rank} IS NULL, {rank}, _rows.{row_id}")
}

pub(super) fn drop_tracking(db: &Connection, key: &str) -> Result<()> {
    for (name, _) in EVENTS {
        let trigger = table::quote(&format!("_cydonia_row_order_{name}_{key}"));
        db.execute_batch(&format!("DROP TRIGGER IF EXISTS {trigger}"))?;
    }
    Ok(())
}

pub(super) fn track_if_ordered(db: &Connection, key: &str) -> Result<()> {
    if ordered(db, key)? {
        track(db, key)?;
    }
    Ok(())
}

fn track(db: &Connection, key: &str) -> Result<()> {
    let rowid = Identity::for_table(db, key, None)?.0;
    let table = table::quote(key);
    let key_literal: String = db.query_row("SELECT quote(?1)", [key], |row| row.get(0))?;
    for (name, event) in EVENTS {
        let trigger = table::quote(&format!("_cydonia_row_order_{name}_{key}"));
        let (condition, affected) = match event {
            "INSERT" => (String::new(), format!("NEW.{rowid}")),
            "DELETE" => (String::new(), format!("OLD.{rowid}")),
            _ => (
                format!("WHEN OLD.{rowid} <> NEW.{rowid}"),
                format!("OLD.{rowid}, NEW.{rowid}"),
            ),
        };
        db.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS {trigger} AFTER {event} ON {table}
             {condition} BEGIN
                 DELETE FROM _cydonia_table_row_order
                 WHERE table_key = {key_literal} AND row_id IN ({affected});
             END;"
        ))?;
    }
    Ok(())
}

impl Data {
    pub fn move_row(
        &mut self,
        table: &str,
        row: i64,
        after: Option<i64>,
        before: Option<i64>,
        revision: Revision,
    ) -> Result<()> {
        let key = self.resolve(table)?;
        let tx = Transaction::new_unchecked(&self.writer, TransactionBehavior::Immediate)?;
        if self.revision()? != revision {
            bail!("table data changed while moving the row");
        }
        let rowid = Identity::for_table(&tx, &key, None)?.0;
        let table = table::quote(&key);
        let order = order_by(rowid, "?1");
        let mut ids = tx
            .prepare(&format!(
                "SELECT _rows.{rowid} FROM {table} AS _rows{order}"
            ))?
            .query_map([&key], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let Some(source) = ids.iter().position(|&id| id == row) else {
            bail!("no row {row} on {key}");
        };
        ids.remove(source);
        let target = before
            .and_then(|before| ids.iter().position(|&id| id == before))
            .or_else(|| {
                after.and_then(|after| ids.iter().position(|&id| id == after).map(|ix| ix + 1))
            });
        let Some(target) = target.filter(|&target| target != source) else {
            return Ok(());
        };
        ids.insert(target, row);
        track(&tx, &key)?;
        let mut upsert = tx.prepare_cached(
            "INSERT INTO _cydonia_table_row_order (table_key, row_id, position) VALUES (?1, ?2, ?3)
             ON CONFLICT(table_key, row_id) DO UPDATE SET position = excluded.position
             WHERE position <> excluded.position",
        )?;
        for (position, id) in ids.into_iter().enumerate() {
            upsert.execute(params![&key, id, position as i64])?;
        }
        drop(upsert);
        tx.commit()?;
        Ok(())
    }
}
