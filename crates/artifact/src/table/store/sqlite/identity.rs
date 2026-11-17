use anyhow::{Result, bail};
use rusqlite::Connection;

const ALIASES: [&str; 3] = ["rowid", "_rowid_", "oid"];

pub(crate) struct Identity(pub &'static str);

impl Identity {
    pub fn for_names(columns: &[String]) -> Result<Self> {
        for alias in ALIASES {
            if !columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(alias))
            {
                return Ok(Self(alias));
            }
        }
        bail!("a table must leave one SQLite row identifier available: rowid, _rowid_, or oid")
    }

    pub fn for_table(
        connection: &Connection,
        key: &str,
        change: Option<(&str, &str)>,
    ) -> Result<Self> {
        let without_rowid: bool = connection.query_row(
            "SELECT wr FROM pragma_table_list WHERE schema = 'main' AND name = ?1",
            [key],
            |row| row.get(0),
        )?;
        if without_rowid {
            bail!("tables without a SQLite row identifier are not supported");
        }
        let mut columns = connection
            .prepare("SELECT name FROM pragma_table_xinfo(?1)")?
            .query_map([key], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if let Some((before, after)) = change {
            if let Some(column) = columns
                .iter_mut()
                .find(|column| column.eq_ignore_ascii_case(before))
            {
                *column = after.to_owned();
            } else {
                columns.push(after.to_owned());
            }
        }
        Self::for_names(&columns)
    }
}
