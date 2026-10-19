mod common;
use common::Wait as _;

use common::Scratch;
use cydonia_artifact::kind::Kind;
use cydonia_artifact::{
    entry::Registry,
    project::{Project as _, store::Store},
};

#[test]
fn numbers_survive_reopen_rename_and_deletion_without_reuse() {
    let scratch = Scratch::new("entry-lifecycle");
    let mut registry = Registry::open(scratch.path()).unwrap();
    let article = registry.number(Kind::Article, "same").unwrap();
    let table = registry.number(Kind::Table, "same").unwrap();
    assert_eq!((article, table), (1, 2));
    registry.rename(Kind::Table, "same", "renamed").unwrap();
    drop(registry);
    let mut registry = Registry::open(scratch.path()).unwrap();
    assert_eq!(registry.number(Kind::Table, "renamed").unwrap(), table);
    assert_eq!(
        registry.resolve(Kind::Article, article).unwrap().as_deref(),
        Some("same")
    );
    assert_eq!(registry.resolve(Kind::Board, article).unwrap(), None);
    registry.remove(Kind::Table, "renamed").unwrap();
    assert_eq!(registry.resolve(Kind::Table, table).unwrap(), None);
    assert!(registry.number(Kind::Table, "renamed").unwrap() > table);
}

#[test]
fn concurrent_connections_allocate_unique_and_consistent_numbers() {
    let scratch = Scratch::new("entry-concurrent");
    let barrier = std::sync::Barrier::new(8);
    let assigned = std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..8)
            .map(|ix| {
                let barrier = &barrier;
                let root = scratch.path();
                scope.spawn(move || {
                    barrier.wait();
                    let mut registry = Registry::open(root).unwrap();
                    let shared = registry.number(Kind::Article, "shared").unwrap();
                    let own = registry.number(Kind::Board, &ix.to_string()).unwrap();
                    (shared, own)
                })
            })
            .collect();
        tasks
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(assigned.iter().all(|(shared, _)| *shared == assigned[0].0));
    let unique: std::collections::HashSet<_> = assigned.iter().map(|(_, own)| *own).collect();
    assert_eq!(unique.len(), 8);
    assert!(!unique.contains(&assigned[0].0));
}

#[test]
fn legacy_content_gets_stable_numbers_and_empty_projects_stay_empty() {
    let scratch = Scratch::new("entry-legacy");
    assert!(Store::Fs(scratch.store()).entries().unwrap().is_empty());
    assert!(!scratch.path().join(".cydonia").exists());
    let root = scratch.store().init().unwrap();
    std::fs::create_dir_all(root.join("boards")).unwrap();
    std::fs::write(root.join("boards/legacy.toml"), "name = \"Old board\"\n").unwrap();
    std::fs::create_dir_all(root.join("articles/legacy")).unwrap();
    std::fs::write(root.join("articles/legacy/content.md"), "old content").unwrap();
    let first = Store::Fs(scratch.store()).entries().unwrap();
    assert_eq!(first.len(), 2);
    assert_ne!(first[0].number, first[1].number);
    let again = Store::Fs(scratch.store()).entries().unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(again).unwrap()
    );
    let board = scratch.store().boards().remove(0);
    assert!(board.number.is_some());
    assert_eq!(
        std::fs::read_to_string(root.join("articles/legacy/content.md")).unwrap(),
        "old content"
    );
}

#[test]
fn archived_sessions_are_discoverable_and_readable() {
    let scratch = Scratch::new("entry-session");
    let root = scratch.store().init().unwrap().join("sessions");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("old.json"), r#"{"agent":"codex","title":"Draft","name":"Saved chat","updated":1,"closed":true,"items":[]}"#).unwrap();
    let catalog = Store::Fs(scratch.store()).entries().unwrap();
    assert_eq!(catalog.len(), 1);
    let session = &catalog[0];
    assert_eq!(session.kind, Kind::Session);
    assert_eq!(session.title, "Saved chat");
    assert!(session.archived);
    assert_eq!(scratch.store().sessions()[0].number, Some(session.number));
    assert_eq!(
        Store::Fs(scratch.store()).read(session).unwrap()["agent"],
        "codex"
    );
    scratch.store().remove_session("old").wait().unwrap();
    assert!(Store::Fs(scratch.store()).entries().unwrap().is_empty());
    assert_eq!(
        Registry::open(scratch.path())
            .unwrap()
            .resolve(Kind::Session, session.number)
            .unwrap(),
        None
    );
}

#[test]
fn table_previews_are_bounded_and_quote_identifiers() {
    let scratch = Scratch::new("entry-preview");
    let root = scratch.store().init().unwrap();
    let connection = rusqlite::Connection::open(root.join("data.db")).unwrap();
    connection.execute_batch("CREATE TABLE \"a\"\"b\" (value TEXT); WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 201) INSERT INTO \"a\"\"b\" SELECT x FROM n;").unwrap();
    let catalog = Store::Fs(scratch.store()).entries().unwrap();
    assert_eq!(catalog.len(), 1);
    let content = Store::Fs(scratch.store()).read(&catalog[0]).unwrap();
    assert_eq!(content["rows"].as_array().unwrap().len(), 200);
    assert_eq!(content["total"], 201);
    assert_eq!(content["key"], "a\"b");
}

#[test]
fn a_blob_cell_previews_as_its_length() {
    let scratch = Scratch::new("entry-blob");
    let root = scratch.store().init().unwrap();
    let connection = rusqlite::Connection::open(root.join("data.db")).unwrap();
    connection
        .execute_batch("CREATE TABLE pics (data BLOB); INSERT INTO pics VALUES (x'00010203');")
        .unwrap();
    let store = Store::Fs(scratch.store());
    let catalog = store.entries().unwrap();
    let content = store.read(&catalog[0]).unwrap();
    assert_eq!(
        content["rows"][0][0],
        serde_json::json!({ "blob_bytes": 4 })
    );
}
