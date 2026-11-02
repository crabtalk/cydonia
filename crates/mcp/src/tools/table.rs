//! Agent tools for project tables.

use crate::{
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{self, PROJECT},
};
use artifact::{
    kind::Kind,
    reference,
    table::{
        ColType, Column,
        rows::Edit,
        store::{Data, Home},
    },
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

const MAX_ROWS: i64 = 1000;
const TABLE: Arg = Arg {
    name: "table",
    about: "The table: its project reference (#12), SQL key, or display name.",
};
const NAME: Arg = Arg {
    name: "name",
    about: "The display name.",
};
const KEY: Arg = Arg {
    name: "key",
    about: "The SQL identifier; omitted when creating to derive one from the name.",
};
const COLUMNS: Arg = Arg {
    name: "columns",
    about: "Columns as objects with name and type (text, number, date, check). Date values are Unix seconds; check values are booleans.",
};
const COLUMN: Arg = Arg {
    name: "column",
    about: "The column name.",
};
const TYPE: Arg = Arg {
    name: "type",
    about: "The column type: text, number, date, or check.",
};
const RENAME: Arg = Arg {
    name: "rename",
    about: "The new column name.",
};
const ROWS: Arg = Arg {
    name: "rows",
    about: "Rows as objects mapping column names to cell values.",
};
const EDITS: Arg = Arg {
    name: "edits",
    about: "Cell edits as objects with rowid, column, and value; null clears a cell.",
};
const ROWIDS: Arg = Arg {
    name: "rowids",
    about: "The row identities returned by table_read or table_insert.",
};
const LIMIT: Arg = Arg {
    name: "limit",
    about: "Rows per page, from 1 to 1000; default 200.",
};
const OFFSET: Arg = Arg {
    name: "offset",
    about: "Rows to skip; default 0.",
};
const SORT: Arg = Arg {
    name: "sort",
    about: "Column to sort by.",
};
const DESC: Arg = Arg {
    name: "descending",
    about: "Sort descending; default false.",
};
const SQL: Arg = Arg {
    name: "sql",
    about: "One read-only SELECT statement using table keys, with rowid selected when row identities are needed. Returns at most 1000 rows.",
};

pub static TOOLS: [Tool; 11] = [
    Tool {
        name: "table_list",
        description: "List project tables with their keys, columns, row counts, and references.",
        writes: false,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT], &[]),
        call: list,
    },
    Tool {
        name: "table_create",
        description: "Create a project table with typed columns.",
        writes: true,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, NAME, COLUMNS], &[KEY]),
        call: create,
    },
    Tool {
        name: "table_read",
        description: "Read a page of table rows, including stable row identities and the total row count.",
        writes: false,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, TABLE], &[LIMIT, OFFSET, SORT, DESC]),
        call: read,
    },
    Tool {
        name: "table_query",
        description: "Query project tables with read-only SQL; returns at most 1000 rows.",
        writes: false,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, SQL], &[]),
        call: query,
    },
    Tool {
        name: "table_rename",
        description: "Change a table display name without changing its SQL key.",
        writes: true,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, TABLE, NAME], &[]),
        call: rename,
    },
    Tool {
        name: "table_write_column",
        description: "Add, rename, or retype a column.",
        writes: true,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, TABLE, COLUMN], &[TYPE, RENAME]),
        call: write_column,
    },
    Tool {
        name: "table_insert",
        description: "Insert rows into a table and return their row identities.",
        writes: true,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, TABLE, ROWS], &[]),
        call: insert,
    },
    Tool {
        name: "table_write_cells",
        description: "Update table cells by row identity in one transaction.",
        writes: true,
        deletes: false,
        schema: |bound| schema(bound, &[PROJECT, TABLE, EDITS], &[]),
        call: write_cells,
    },
    Tool {
        name: "table_delete_rows",
        description: "Permanently delete table rows by row identity.",
        writes: true,
        deletes: true,
        schema: |bound| schema(bound, &[PROJECT, TABLE, ROWIDS], &[]),
        call: delete_rows,
    },
    Tool {
        name: "table_delete_column",
        description: "Permanently delete a column and its values.",
        writes: true,
        deletes: true,
        schema: |bound| schema(bound, &[PROJECT, TABLE, COLUMN], &[]),
        call: delete_column,
    },
    Tool {
        name: "table_delete",
        description: "Permanently delete a table and all its rows.",
        writes: true,
        deletes: true,
        schema: |bound| schema(bound, &[PROJECT, TABLE], &[]),
        call: remove,
    },
];

fn schema(bound: bool, required: &[Arg], optional: &[Arg]) -> Value {
    let mut schema = tools::fields(bound, required);
    for arg in optional {
        schema["properties"][arg.name] = json!({"type": "string", "description": arg.about});
    }
    for arg in required.iter().chain(optional) {
        let kind = match arg.name {
            "columns" | "rows" | "edits" | "rowids" => "array",
            "limit" | "offset" => "integer",
            "descending" => "boolean",
            _ => continue,
        };
        schema["properties"][arg.name]["type"] = json!(kind);
        if kind == "array" {
            schema["properties"][arg.name]["items"] = match arg.name {
                "columns" => {
                    json!({"type":"object","properties":{"name":{"type":"string"},"type":{"type":"string","enum":["text","number","date","check"]}},"required":["name","type"]})
                }
                "edits" => {
                    json!({"type":"object","properties":{"rowid":{"type":"integer"},"column":{"type":"string"},"value":{}},"required":["rowid","column","value"]})
                }
                "rowids" => json!({"type":"integer"}),
                _ => {
                    json!({"type":"object","additionalProperties":{"type":["string","number","boolean","null"]}})
                }
            };
        }
    }
    schema
}

fn decode<T: DeserializeOwned>(args: &Args<'_>, arg: Arg) -> Result<T, Trouble> {
    let value = args
        .value(arg)
        .ok_or_else(|| Trouble::Invalid(format!("{} is required", arg.name)))?;
    serde_json::from_value(value.clone())
        .map_err(|e| Trouble::Invalid(format!("{}: {e}", arg.name)))
}

fn refused(error: anyhow::Error) -> Trouble {
    Trouble::Refused(error.to_string())
}

fn answer(value: impl Serialize) -> Outcome {
    let value = serde_json::to_value(value).map_err(|e| Trouble::Refused(e.to_string()))?;
    Ok(Answer::said(serde_json::to_string_pretty(&value).unwrap_or_default()).with(value))
}

fn home(args: &Args<'_>) -> Result<(Home, Option<String>), Trouble> {
    if let Some(named) = args.maybe(TABLE)
        && reference::parse(named).is_some()
    {
        let (project, number, within) = tools::entry_of(args, named)?;
        let entry = tools::entry_numbered(&project, number, named)?;
        reference::fits(named, entry.kind, within)?;
        if entry.kind != Kind::Table {
            return Err(Trouble::Invalid(format!("{named} is not a table")));
        }
        return Ok((Home::Project(project), Some(entry.id)));
    }
    Ok((
        Home::Project(tools::root(args)?.to_path_buf()),
        args.maybe(TABLE).map(str::to_owned),
    ))
}

fn existing(args: &Args<'_>) -> Result<(Data, String), Trouble> {
    let (home, named) = home(args)?;
    let data = Data::attach(&home)
        .ok_or_else(|| Trouble::Refused("this project has no table database".into()))?;
    let named = named.ok_or_else(|| Trouble::Invalid("table is required".into()))?;
    let key = data.resolve(&named).map_err(refused)?;
    Ok((data, key))
}

fn list(args: Args<'_>) -> Outcome {
    let (home, _) = home(&args)?;
    match Data::attach(&home) {
        Some(data) => answer(data.list().map_err(refused)?),
        None => answer(Vec::<Value>::new()),
    }
}

fn create(args: Args<'_>) -> Outcome {
    let name = args.text(NAME)?;
    let columns: Vec<Column> = decode(&args, COLUMNS)?;
    let (home, _) = home(&args)?;
    let mut data = Data::open(&home).map_err(refused)?;
    answer(
        data.create(name, args.maybe(KEY), &columns, None)
            .map_err(refused)?,
    )
}

fn read(args: Args<'_>) -> Outcome {
    let limit = args
        .value(LIMIT)
        .map(|_| decode::<i64>(&args, LIMIT))
        .transpose()?
        .unwrap_or(200);
    let offset = args
        .value(OFFSET)
        .map(|_| decode::<i64>(&args, OFFSET))
        .transpose()?
        .unwrap_or(0);
    if !(1..=MAX_ROWS).contains(&limit) || offset < 0 {
        return Err(Trouble::Invalid(
            "limit must be 1..1000 and offset must be nonnegative".into(),
        ));
    }
    let (data, key) = existing(&args)?;
    answer(
        data.read(
            &key,
            args.maybe(SORT),
            args.boolean(DESC, false)?,
            limit,
            offset,
        )
        .map_err(refused)?,
    )
}

fn query(args: Args<'_>) -> Outcome {
    let sql = args.text(SQL)?.trim().trim_end_matches(';');
    let (home, _) = home(&args)?;
    let data = Data::attach(&home)
        .ok_or_else(|| Trouble::Refused("this project has no table database".into()))?;
    answer(
        data.query(&format!("SELECT * FROM ({sql}) LIMIT {MAX_ROWS}"))
            .map_err(refused)?,
    )
}

fn rename(args: Args<'_>) -> Outcome {
    let (mut data, key) = existing(&args)?;
    answer(
        data.update(&key, Some(args.text(NAME)?), None)
            .map_err(refused)?,
    )
}

fn write_column(args: Args<'_>) -> Outcome {
    let kind: Option<ColType> = args.value(TYPE).map(|_| decode(&args, TYPE)).transpose()?;
    let (mut data, key) = existing(&args)?;
    let columns = data
        .write_column(&key, args.text(COLUMN)?, kind, args.maybe(RENAME))
        .map_err(refused)?;
    data.touch(&key).map_err(refused)?;
    answer(columns)
}

fn insert(args: Args<'_>) -> Outcome {
    let rows: Vec<Map<String, Value>> = decode(&args, ROWS)?;
    let (mut data, key) = existing(&args)?;
    let ids = data.insert(&key, &rows).map_err(refused)?;
    data.touch(&key).map_err(refused)?;
    answer(json!({"rowids":ids}))
}

fn write_cells(args: Args<'_>) -> Outcome {
    let edits: Vec<Edit> = decode(&args, EDITS)?;
    let (mut data, key) = existing(&args)?;
    data.write_cells(&key, &edits).map_err(refused)?;
    data.touch(&key).map_err(refused)?;
    answer(json!({"updated": edits.len()}))
}

fn delete_rows(args: Args<'_>) -> Outcome {
    let ids: Vec<i64> = decode(&args, ROWIDS)?;
    let (mut data, key) = existing(&args)?;
    data.delete_rows(&key, &ids).map_err(refused)?;
    data.touch(&key).map_err(refused)?;
    answer(json!({"deleted":ids.len()}))
}

fn delete_column(args: Args<'_>) -> Outcome {
    let (mut data, key) = existing(&args)?;
    let columns = data
        .drop_column(&key, args.text(COLUMN)?)
        .map_err(refused)?;
    data.touch(&key).map_err(refused)?;
    answer(columns)
}

fn remove(args: Args<'_>) -> Outcome {
    let (mut data, key) = existing(&args)?;
    data.remove(&key).map_err(refused)?;
    answer(json!({"deleted":key}))
}
