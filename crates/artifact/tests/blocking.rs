//! A card's blockers and the cards it blocks change as one: each edit the
//! module answers lands on the card at the other end.

use cydonia_artifact::board::{
    Board,
    blocking::{self, Found, Place, Shelf},
};
use cydonia_artifact::reference;
use std::path::{Path, PathBuf};

/// Boards by the project holding them.
struct Boards(Vec<(PathBuf, Board)>);

impl Shelf for Boards {
    fn project(&self, from: &Path, name: Option<&str>) -> Option<PathBuf> {
        match name {
            None => Some(from.to_path_buf()),
            Some(name) => self
                .0
                .iter()
                .map(|(path, _)| path)
                .find(|path| reference::names(path, name))
                .cloned(),
        }
    }

    fn card(&self, project: &Path, key: &str, handle: u64) -> Option<Found> {
        let (_, board) = self
            .0
            .iter()
            .find(|(path, board)| path == project && board.keyed(key))?;
        Some(Found {
            board: board.id.clone(),
            key: board.key.clone(),
            card: board.card_at(handle)?.id.clone(),
        })
    }
}

impl Boards {
    fn apply(&mut self, edits: &[blocking::Edit]) {
        for (path, board) in &mut self.0 {
            blocking::apply(edits, path, board);
        }
    }

    fn card(&self, project: &str, key: &str, handle: u64) -> &cydonia_artifact::board::Card {
        let (_, board) = self
            .0
            .iter()
            .find(|(path, board)| path.ends_with(project) && board.keyed(key))
            .expect("a board");
        board.card_at(handle).expect("a card")
    }

    fn place(&self, project: &str, key: &str, handle: u64) -> (Place, String, String) {
        let (path, board) = self
            .0
            .iter()
            .find(|(path, board)| path.ends_with(project) && board.keyed(key))
            .expect("a board");
        let card = board.card_at(handle).expect("a card");
        (
            Place::of(path, board, card).expect("a place"),
            board.id.clone(),
            card.id.clone(),
        )
    }
}

/// A board keyed `key` holding `cards` cards in one lane.
fn board(id: &str, key: &str, cards: usize) -> Board {
    let mut board = Board::new(id.into(), key);
    board.key = key.into();
    let lane = board.add_column("Todo").id.clone();
    for n in 0..cards {
        board.add_card(&lane, format!("card {n}"));
    }
    board
}

fn two_projects() -> Boards {
    Boards(vec![
        (PathBuf::from("/work/one"), board("dev", "DEV", 3)),
        (PathBuf::from("/work/one"), board("ops", "OPS", 1)),
        (PathBuf::from("/work/two"), board("web", "WEB", 1)),
    ])
}

fn set(boards: &mut Boards, waiter: (&str, &str, u64), wanted: &[&str]) {
    let (place, board, card) = boards.place(waiter.0, waiter.1, waiter.2);
    let held = boards.card(waiter.0, waiter.1, waiter.2).blocked_by.clone();
    let wanted: Vec<String> = wanted.iter().map(|text| text.to_string()).collect();
    let edits =
        blocking::set_blockers(boards, &place, &board, &card, &held, &wanted).expect("found");
    boards.apply(&edits);
}

/// Each blocker once, never the card itself, and named back on each.
#[test]
fn a_blocker_names_its_waiter_back_as_its_project_writes_it() {
    let mut boards = two_projects();
    set(
        &mut boards,
        ("one", "DEV", 1),
        &["DEV-2", "DEV-1", "DEV-2", "two#WEB-1"],
    );

    assert_eq!(
        boards.card("one", "DEV", 1).blocked_by,
        ["DEV-2", "two#WEB-1"]
    );
    assert_eq!(boards.card("one", "DEV", 2).blocks, ["DEV-1"]);
    assert_eq!(boards.card("two", "WEB", 1).blocks, ["one#DEV-1"]);

    set(&mut boards, ("one", "DEV", 1), &["two#WEB-1"]);
    assert!(boards.card("one", "DEV", 2).blocks.is_empty());
    assert_eq!(boards.card("two", "WEB", 1).blocks, ["one#DEV-1"]);
}

#[test]
fn a_blocker_that_names_no_card_refuses_the_change() {
    let mut boards = two_projects();
    let (place, board, card) = boards.place("one", "DEV", 1);
    let refused =
        blocking::set_blockers(&boards, &place, &board, &card, &[], &["DEV-9".to_owned()]);
    assert_eq!(refused, Err("DEV-9".to_owned()));
    set(&mut boards, ("one", "DEV", 1), &[]);
    assert!(boards.card("one", "DEV", 1).blocked_by.is_empty());
}

#[test]
fn a_card_going_is_forgotten_at_both_ends() {
    let mut boards = two_projects();
    set(&mut boards, ("one", "DEV", 1), &["DEV-2"]);
    set(&mut boards, ("one", "DEV", 3), &["DEV-1"]);

    let (place, ..) = boards.place("one", "DEV", 1);
    let gone = boards.card("one", "DEV", 1).clone();
    let others = blocking::counterparts(&boards, &place.project, &gone);
    boards.apply(&blocking::forget(&place, &others));

    assert!(boards.card("one", "DEV", 2).blocks.is_empty());
    assert!(boards.card("one", "DEV", 3).blocked_by.is_empty());
}

#[test]
fn a_card_carried_to_another_project_is_named_anew() {
    let mut boards = two_projects();
    set(&mut boards, ("one", "DEV", 1), &["DEV-2"]);
    set(&mut boards, ("one", "DEV", 3), &["DEV-1"]);

    let (from, ..) = boards.place("one", "DEV", 1);
    let card = boards.card("one", "DEV", 1).clone();
    let others = blocking::counterparts(&boards, &from.project, &card);
    let (to, board, id) = boards.place("two", "WEB", 1);
    boards.apply(&blocking::moved(&from, &to, &board, &id, &others));

    assert_eq!(boards.card("one", "DEV", 2).blocks, ["two#WEB-1"]);
    assert_eq!(boards.card("one", "DEV", 3).blocked_by, ["two#WEB-1"]);
    let landed = boards.card("two", "WEB", 1);
    assert_eq!(landed.blocked_by, ["one#DEV-2"]);
    assert_eq!(landed.blocks, ["one#DEV-3"]);
}

#[test]
fn a_rekeyed_board_is_named_anew_on_its_own_cards() {
    let mut boards = two_projects();
    set(&mut boards, ("one", "DEV", 1), &["DEV-2", "OPS-1"]);

    let before: Vec<_> = (1..=2)
        .map(|handle| {
            let (place, board, id) = boards.place("one", "DEV", handle);
            let card = boards.card("one", "DEV", handle).clone();
            let others = blocking::counterparts(&boards, &place.project, &card);
            (place, board, id, others)
        })
        .collect();
    let mut edits = Vec::new();
    for (from, board, id, others) in &before {
        let to = Place {
            key: "CORE".into(),
            ..from.clone()
        };
        edits.extend(blocking::moved(from, &to, board, id, others));
    }
    boards.0[0].1.key = "CORE".into();
    boards.apply(&edits);

    assert_eq!(
        boards.card("one", "CORE", 1).blocked_by,
        ["CORE-2", "OPS-1"]
    );
    assert_eq!(boards.card("one", "CORE", 2).blocks, ["CORE-1"]);
    assert_eq!(boards.card("one", "OPS", 1).blocks, ["CORE-1"]);
}

/// A wait written before `blocks` was kept is named back the next time its
/// list is set, unchanged.
#[test]
fn setting_the_same_blockers_names_them_back() {
    let mut boards = two_projects();
    boards.0[0]
        .1
        .columns
        .iter_mut()
        .flat_map(|column| &mut column.cards)
        .find(|card| card.handle == Some(1))
        .unwrap()
        .blocked_by = vec!["DEV-2".into()];
    set(&mut boards, ("one", "DEV", 1), &["DEV-2"]);
    assert_eq!(boards.card("one", "DEV", 2).blocks, ["DEV-1"]);
}
