#![cfg(feature = "sqlite")]

use cydonia_artifact::table::{
    ColType, Column,
    rows::Edit,
    store::{Data, Home},
};

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
