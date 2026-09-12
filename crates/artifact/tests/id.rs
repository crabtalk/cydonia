//! Every entry comes back named, including one written before ids existed.

use cydonia_artifact::{project::Project as _, session::record::Record};
use std::fs;

mod common;

use common::Scratch;

#[test]
fn a_new_board_is_named_when_it_is_made() {
    let scratch = Scratch::new("board-new");
    let board = scratch.store().create_board().expect("made");
    assert!(!board.id.is_empty());
    assert_eq!(scratch.store().boards()[0].id, board.id);
}

/// The whole of the migration: a board file with no `id` key is read back with
/// the one it has always had — the name of the file it is in.
#[test]
fn a_board_written_before_ids_takes_the_name_of_its_file() {
    let scratch = Scratch::new("board-old");
    let dir = scratch.store().init().unwrap().join("boards");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("1757000000000.toml"), "name = \"Roadmap\"\n").unwrap();

    let boards = scratch.store().boards();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].id, "1757000000000");
    assert_eq!(boards[0].name, "Roadmap");
}

/// Reading writes back only what it minted. A board already carrying
/// everything — its id, its key, its counter — is left alone: `boards` runs on
/// every re-read of a project, and a write from inside one is an event the
/// watch answers by re-reading again.
#[test]
fn reading_a_settled_board_leaves_the_file_alone() {
    let scratch = Scratch::new("board-quiet");
    let dir = scratch.store().init().unwrap().join("boards");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("1757000000000.toml");
    let before = "id = \"1757000000000\"\narchived = false\nname = \"Roadmap\"\nkey = \"ROA\"\nnext_handle = 1\ncolumns = []\n";
    fs::write(&file, before).unwrap();

    scratch.store().boards();
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

/// Two sessions minted in the same millisecond get `<stamp>` and `<stamp>-2`,
/// and the stem keeps them apart — so the ids do too.
#[test]
fn sessions_made_together_are_named_apart() {
    let scratch = Scratch::new("session-pair");
    let store = scratch.store();
    let one = store.create_session().expect("minted");
    store.save_session(&record(&one));
    let two = store.create_session().expect("minted");

    assert_ne!(one, two);
    assert_eq!(store.sessions().len(), 1, "only the written one is filed");
}

/// A session written before ids existed comes back named by its file.
#[test]
fn a_session_written_before_ids_takes_the_name_of_its_file() {
    let scratch = Scratch::new("session-old");
    let dir = scratch.store().init().unwrap().join("sessions");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("1757000000000-2.json"),
        r#"{"agent":"claude","title":"Ship it","name":null,"updated":1757000000,"items":[]}"#,
    )
    .unwrap();

    let sessions = scratch.store().sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "1757000000000-2");
    assert_eq!(sessions[0].title, "Ship it");
}

/// The least a session can be filed as.
fn record(id: &str) -> Record {
    Record {
        id: id.to_owned(),
        agent: "claude".into(),
        session: None,
        title: "Ship it".into(),
        name: None,
        updated: 1_757_000_000,
        closed: false,
        items: Vec::new(),
    }
}
