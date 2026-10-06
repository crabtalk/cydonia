mod common;

use common::Scratch;
use cydonia_artifact::{
    board::{self, Status},
    project::{Project as _, Stale},
};
use std::fs;

/// A board and its column, with two cards on it.
fn board_with_two(scratch: &Scratch) -> (String, String, String) {
    let store = scratch.store();
    let mut made = store.create_board("Work", "WORK").unwrap();
    let column = made.add_column("Todo").id.clone();
    let one = made.add_card(&column, "one".into()).unwrap().id.clone();
    let two = made.add_card(&column, "two".into()).unwrap().id.clone();
    store.save_board(&mut made).unwrap();
    (made.id, one, two)
}

#[test]
fn a_flat_board_moves_into_its_directory() {
    let scratch = Scratch::new("layout-flat");
    let boards = scratch.store().init().unwrap().join("boards");
    fs::create_dir_all(&boards).unwrap();
    fs::write(
        boards.join("1757000000000.toml"),
        "name = \"Old\"\nkey = \"OLD\"\nnext_handle = 3\n\n[[columns]]\nid = \"c\"\nname = \"TODO\"\n\n[[columns.cards]]\nid = \"a\"\nhandle = 1\ntext = \"first\"\nstatus = \"busy\"\n\n[[columns.cards]]\nid = \"b\"\nhandle = 2\ntext = \"second\\nline\"\n",
    )
    .unwrap();

    let read = scratch.store().boards();
    assert_eq!(read.len(), 1);
    let dir = boards.join("1757000000000");
    assert!(!boards.join("1757000000000.toml").exists());
    assert!(dir.join("board.toml").is_file());
    assert_eq!(
        fs::read_to_string(dir.join("cards/a.md")).unwrap(),
        "+++\nhandle = 1\nstatus = \"busy\"\n+++\nfirst"
    );
    let cards = &read[0].columns[0].cards;
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[1].text, "second\nline");
    assert_eq!(read[0].next_handle, 3);
}

#[test]
fn edits_to_different_cards_do_not_clash() {
    let scratch = Scratch::new("layout-apart");
    let (id, one, two) = board_with_two(&scratch);
    let store = scratch.store();
    let mut first = store.board(&id).unwrap();
    let mut second = store.board(&id).unwrap();
    first.set_card_status(&one, Some(Status::Done));
    store.save_board(&mut first).unwrap();
    second.set_card_status(&two, Some(Status::Busy));
    store.save_board(&mut second).unwrap();

    let now = store.board(&id).unwrap();
    assert_eq!(now.card(&one).unwrap().status, Some(Status::Done));
    assert_eq!(now.card(&two).unwrap().status, Some(Status::Busy));
}

#[test]
fn edits_to_the_same_card_are_stale() {
    let scratch = Scratch::new("layout-same");
    let (id, one, _) = board_with_two(&scratch);
    let store = scratch.store();
    let mut first = store.board(&id).unwrap();
    let mut second = store.board(&id).unwrap();
    first.set_card_status(&one, Some(Status::Done));
    store.save_board(&mut first).unwrap();
    second.set_card_status(&one, Some(Status::Blocked));
    assert!(store.save_board(&mut second).unwrap_err().is::<Stale>());
}

#[test]
fn a_card_edit_survives_a_reorder_elsewhere_but_not_its_removal() {
    let scratch = Scratch::new("layout-order");
    let (id, one, two) = board_with_two(&scratch);
    let store = scratch.store();
    let mut mover = store.board(&id).unwrap();
    let mut editor = store.board(&id).unwrap();
    let column = mover.columns[0].id.clone();
    mover.move_card_before(&two, &column, Some(&one));
    store.save_board(&mut mover).unwrap();
    editor.set_card_status(&one, Some(Status::Done));
    store.save_board(&mut editor).unwrap();
    let now = store.board(&id).unwrap();
    assert_eq!(now.columns[0].cards[0].id, two);
    assert_eq!(now.card(&one).unwrap().status, Some(Status::Done));

    let mut remover = store.board(&id).unwrap();
    let mut late = store.board(&id).unwrap();
    remover.remove_card(&one);
    store.save_board(&mut remover).unwrap();
    let file = scratch
        .path()
        .join(".cydonia/boards")
        .join(&id)
        .join("cards")
        .join(format!("{one}.md"));
    assert!(!file.exists());
    late.set_card_status(&one, Some(Status::Busy));
    assert!(store.save_board(&mut late).unwrap_err().is::<Stale>());
}

#[test]
fn a_moved_card_keeps_its_id() {
    let scratch = Scratch::new("layout-carry");
    let (id, one, _) = board_with_two(&scratch);
    let store = scratch.store();
    let mut from = store.board(&id).unwrap();
    let mut to = store.create_board("Plan", "PLAN").unwrap();
    to.add_column("Todo");
    board::carry_card(&mut from, &mut to, &one, None).unwrap();
    store.save_board(&mut to).unwrap();
    store.save_board(&mut from).unwrap();
    assert!(store.board(&to.id).unwrap().card(&one).is_some());
    assert!(store.board(&id).unwrap().card(&one).is_none());
}
