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
