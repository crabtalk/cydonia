#![cfg(feature = "sqlite")]

use cydonia_artifact::table::{
    ColType, Column,
    rows::Edit,
    store::{Data, Home},
};

#[test]
fn manual_order_survives_schema_changes_and_does_not_follow_reused_row_ids() {
    let path = std::env::temp_dir().join(format!("cydonia-row-order-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let column = Column {
        name: "Name".into(),
        kind: ColType::Text,
    };
    let key = data.create("Tasks", None, &[column], None).unwrap().key;
    let ids = data.add_rows(&key, 3).unwrap();
    let revision = data.read(&key, None, false, 3, 0).unwrap().revision;
    data.move_row(&key, ids[2], None, Some(ids[0]), revision)
        .unwrap();
    let key = data.update(&key, None, Some("Moved")).unwrap().key;
    data.write_column(&key, "rowid", Some(ColType::Text), None)
        .unwrap();
    data.write_column(&key, "Name", Some(ColType::Number), None)
        .unwrap();
    let order = |data: &Data| {
        data.read(&key, None, false, 3, 0)
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.rowid)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&data), [ids[2], ids[0], ids[1]]);
    data.write("DELETE FROM moved WHERE _rowid_ = 3; INSERT INTO moved DEFAULT VALUES")
        .unwrap();
    assert_eq!(order(&data), ids);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn an_external_table_can_be_archived_without_metadata() {
    let path = std::env::temp_dir().join(format!(
        "cydonia-external-table-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let connection = rusqlite::Connection::open(path.join("data.db")).unwrap();
    connection
        .execute_batch("CREATE TABLE external (name TEXT)")
        .unwrap();
    data.archive("external", true).unwrap();
    assert!(data.list().unwrap()[0].archived);
    data.archive("external", false).unwrap();
    assert!(!data.list().unwrap()[0].archived);
    assert!(data.archive("missing", true).is_err());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn writing_a_missing_row_rejects_the_whole_batch() {
    let path = std::env::temp_dir().join(format!(
        "cydonia-missing-row-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let key = data
        .create(
            "Tasks",
            None,
            &[Column {
                name: "Name".into(),
                kind: ColType::Text,
            }],
            None,
        )
        .unwrap()
        .key;
    let rowid = data.add_rows(&key, 1).unwrap()[0];
    assert!(
        data.write_cells(
            &key,
            &[
                Edit {
                    rowid,
                    column: "Name".into(),
                    value: "Changed".into()
                },
                Edit {
                    rowid: rowid + 1,
                    column: "Name".into(),
                    value: "Gone".into()
                },
            ]
        )
        .is_err()
    );
    assert!(data.read(&key, None, false, 10, 0).unwrap().rows[0].cells[0].is_null());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn retyping_preserves_tables_with_the_scratch_name() {
    let path = std::env::temp_dir().join(format!("cydonia-rebuild-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let columns = [Column {
        name: "Name".into(),
        kind: ColType::Text,
    }];
    let key = data.create("Tasks", None, &columns, None).unwrap().key;
    let other = data
        .create("Tasks rebuilding", None, &columns, None)
        .unwrap()
        .key;
    let ids = data.add_rows(&other, 1).unwrap();
    data.write_cells(
        &other,
        &[Edit {
            rowid: ids[0],
            column: "Name".into(),
            value: "Keep".into(),
        }],
    )
    .unwrap();
    data.write_column(&key, "Name", Some(ColType::Number), None)
        .unwrap();
    assert_eq!(
        data.read(&other, None, false, 10, 0).unwrap().rows[0].cells[0],
        "Keep"
    );
    assert_eq!(data.list().unwrap().len(), 2);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn retyping_preserves_external_schema_and_rolls_back_failures() {
    let path = std::env::temp_dir().join(format!("cydonia-schema-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let connection = rusqlite::Connection::open(path.join("data.db")).unwrap();
    connection.execute_batch("CREATE TABLE external (name TEXT NOT NULL UNIQUE DEFAULT 'kept', amount TEXT CHECK(amount <> 'bad'), calculated TEXT GENERATED ALWAYS AS (name || amount) VIRTUAL); CREATE INDEX external_amount ON external(amount); CREATE TABLE audit(value TEXT); CREATE TRIGGER external_changed AFTER UPDATE ON external BEGIN INSERT INTO audit VALUES(new.name); END; INSERT INTO external(name, amount) VALUES('first', '12')").unwrap();
    data.write_column("external", "amount", Some(ColType::Number), None)
        .unwrap();
    assert_eq!(connection.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('external_amount', 'external_changed')", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
    let rowid = data.add_rows("external", 1).unwrap()[0];
    assert_eq!(
        connection
            .query_row("SELECT name FROM external WHERE rowid = ?1", [rowid], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "kept"
    );
    assert!(
        connection
            .execute("INSERT INTO external(name) VALUES(NULL)", [])
            .is_err()
    );
    assert!(
        connection
            .execute("INSERT INTO external(name) VALUES('kept')", [])
            .is_err()
    );
    connection
        .execute(
            "UPDATE external SET name = 'changed' WHERE rowid = ?1",
            [rowid],
        )
        .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT value FROM audit", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "changed"
    );
    let before: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name = 'external'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        data.write_column("external", "amount", Some(ColType::Text), Some("name"))
            .is_err()
    );
    let after: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name = 'external'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        data.read("external", None, false, 10, 0)
            .unwrap()
            .rows
            .len(),
        2
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn failed_retyping_preserves_schema_data_and_column_name() {
    let path = std::env::temp_dir().join(format!("cydonia-schema-rollback-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let connection = rusqlite::Connection::open(path.join("data.db")).unwrap();
    connection.execute_batch("CREATE TABLE external (name TEXT UNIQUE); INSERT INTO external VALUES('01'), ('1'); CREATE INDEX external_name ON external(name)").unwrap();
    assert!(
        data.write_column("external", "name", Some(ColType::Number), Some("Renamed"))
            .is_err()
    );
    let page = data.read("external", None, false, 10, 0).unwrap();
    assert_eq!(page.columns[0].name, "name");
    assert_eq!(page.rows[0].cells[0], "01");
    assert_eq!(page.rows[1].cells[0], "1");
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'external_name'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn checked_edits_reject_reused_ids_after_raw_writes_and_reopening() {
    let path = std::env::temp_dir().join(format!("cydonia-row-conflict-{}", std::process::id()));
    let home = Home::App(path.clone());
    let mut data = Data::open(&home).unwrap();
    data.create(
        "Tasks",
        None,
        &[Column {
            name: "Name".into(),
            kind: ColType::Text,
        }],
        None,
    )
    .unwrap();
    let id = data.add_rows("tasks", 1).unwrap()[0];
    for external in [false, true] {
        let page = data.read("tasks", None, false, 10, 0).unwrap();
        let sql = "DELETE FROM tasks; INSERT INTO tasks(Name) VALUES('replacement')";
        if external {
            Data::open(&home).unwrap().write(sql).unwrap();
        } else {
            data.write(sql).unwrap();
        }
        assert_eq!(
            data.read("tasks", None, false, 10, 0).unwrap().rows[0].rowid,
            id
        );
        assert!(
            data.write_cells_at(
                "tasks",
                &[Edit {
                    rowid: id,
                    column: "Name".into(),
                    value: "stale".into()
                }],
                Some(page.revision)
            )
            .is_err()
        );
        assert_eq!(
            data.read("tasks", None, false, 10, 0).unwrap().rows[0].cells[0],
            "replacement"
        );
    }
    let page = data.read("tasks", None, false, 10, 0).unwrap();
    data.write_cells_at(
        "tasks",
        &[Edit {
            rowid: id,
            column: "Name".into(),
            value: "fresh".into(),
        }],
        Some(page.revision),
    )
    .unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn rowid_named_columns_keep_reads_edits_deletes_and_retyping_usable() {
    let path = std::env::temp_dir().join(format!("cydonia-rowid-alias-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    for alias in ["rowid", "_rowid_", "OID"] {
        let key = data
            .create(
                "Tasks",
                None,
                &[Column {
                    name: "Name".into(),
                    kind: ColType::Text,
                }],
                None,
            )
            .unwrap()
            .key;
        let id = data.add_rows(&key, 1).unwrap()[0];
        data.write_cells(
            &key,
            &[Edit {
                rowid: id,
                column: "Name".into(),
                value: "keep".into(),
            }],
        )
        .unwrap();
        data.write_column(&key, "Name", None, Some(alias)).unwrap();
        let page = data.read(&key, None, false, 10, 0).unwrap();
        assert_eq!(page.rows[0].rowid, id);
        assert_eq!(page.rows[0].cells[0], "keep");
        data.write_column(&key, alias, Some(ColType::Number), None)
            .unwrap();
        data.write_cells(
            &key,
            &[Edit {
                rowid: id,
                column: alias.into(),
                value: "updated".into(),
            }],
        )
        .unwrap();
        let page = data.read(&key, None, false, 10, 0).unwrap();
        assert_eq!(page.rows[0].rowid, id);
        assert_eq!(page.rows[0].cells[0], "updated");
        data.delete_rows(&key, &[id]).unwrap();
        assert!(data.read(&key, None, false, 10, 0).unwrap().rows.is_empty());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn schema_changes_cannot_shadow_every_row_identifier() {
    let path = std::env::temp_dir().join(format!("cydonia-rowid-guard-{}", std::process::id()));
    let mut data = Data::open(&Home::App(path.clone())).unwrap();
    let columns: Vec<_> = ["rowid", "_rowid_", "Name"]
        .into_iter()
        .map(|name| Column {
            name: name.into(),
            kind: ColType::Text,
        })
        .collect();
    data.create("Tasks", None, &columns, None).unwrap();
    let id = data.add_rows("tasks", 1).unwrap()[0];
    assert!(
        data.write_column("tasks", "oid", Some(ColType::Text), None)
            .is_err()
    );
    assert!(
        data.write_column("tasks", "Name", None, Some("OID"))
            .is_err()
    );
    assert!(
        data.write_column("tasks", "Name", Some(ColType::Number), Some("oid"))
            .is_err()
    );
    let mut invalid = columns.clone();
    invalid[2].name = " oid ".into();
    assert!(data.create("Invalid", None, &invalid, None).is_err());
    assert_eq!(data.list().unwrap().len(), 1);
    let page = data.read("tasks", None, false, 10, 0).unwrap();
    assert_eq!(page.rows[0].rowid, id);
    assert_eq!(page.columns[2].name, "Name");
    assert_eq!(page.columns[2].kind, ColType::Text);
    let connection = rusqlite::Connection::open(path.join("data.db")).unwrap();
    connection.execute_batch("CREATE TABLE generated (value TEXT, rowid TEXT GENERATED ALWAYS AS (value) VIRTUAL, _rowid_ TEXT); INSERT INTO generated(value) VALUES('kept')").unwrap();
    assert!(
        data.write_column("generated", "value", None, Some("oid"))
            .is_err()
    );
    data.write_column("generated", "value", Some(ColType::Number), None)
        .unwrap();
    assert_eq!(
        data.read("generated", None, false, 10, 0).unwrap().rows[0].cells[0],
        "kept"
    );
    std::fs::remove_dir_all(path).unwrap();
}
