mod common;

use common::Scratch;
use cydonia_artifact::table::{ColType, Column, Home, Local as _, sqlite::Sqlite};

#[test]
fn table_reference_survives_key_changes_and_is_retired_on_delete() {
    let scratch = Scratch::new("table");
    let home = Home::Project(scratch.path().to_owned());
    let mut data = Sqlite::open(&home).unwrap();
    let columns = [Column {
        name: "text".into(),
        kind: ColType::Text,
    }];
    let first = data.create("Notes", Some("notes"), &columns, None).unwrap();
    let reference = format!("#{}", first.number.unwrap());
    let renamed = data
        .update(&reference, Some("Final"), Some("final_notes"))
        .unwrap();
    assert_eq!(first.number, renamed.number);
    drop(data);
    let mut data = Sqlite::open(&home).unwrap();
    assert_eq!(data.list().unwrap()[0].number, first.number);
    assert!(data.read(&reference, None, false, 200, 0).is_ok());
    data.remove(&reference).unwrap();
    let replacement = data
        .create("Final", Some("final_notes"), &columns, None)
        .unwrap();
    assert_ne!(replacement.number, first.number);
    assert!(data.read(&reference, None, false, 200, 0).is_err());
}
