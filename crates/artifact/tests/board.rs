//! A board comes back addressable, stays that way between reads, and is only
//! ever changed by something with a name.

mod common;

use common::Scratch;
use cydonia_artifact::{
    board::{Board, Card, Column},
    project::Project as _,
};
use std::{collections::HashSet, fs};

/// A board opens empty. Todo/Doing/Done at the constructor was a guess about
/// the work, compiled in where no one could change it.
#[test]
fn a_new_board_has_no_columns() {
    let board = Board::new("1757000000000".into(), "Roadmap");
    assert!(board.columns.is_empty());
}

/// Lanes and cards made in one go are still told apart. They are minted from
/// the millisecond, and a run of them takes less than one.
#[test]
fn everything_made_together_is_named_apart() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let todo = board.add_column("Todo").id.clone();
    let doing = board.add_column("Doing").id.clone();
    for n in 0..8 {
        board.add_card(&todo, format!("card {n}")).expect("a lane");
    }

    assert_ne!(todo, doing);
    let found = ids(&board);
    assert_eq!(
        found.iter().collect::<HashSet<_>>().len(),
        found.len(),
        "{found:?}"
    );
}

/// Everything a read finds unnamed gets a name, clear of the names already in
/// the file.
#[test]
fn minting_names_what_has_no_name() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    board.columns = vec![unnamed("Todo", 4), unnamed("Doing", 4)];
    board.columns[1].id = "doing".into();

    assert!(board.mint_ids(), "there was something to name");
    assert!(!board.mint_ids(), "and nothing left to name after");

    let found = ids(&board);
    assert!(found.iter().all(|id| !id.is_empty()), "{found:?}");
    assert!(found.contains(&"doing".to_owned()), "kept the one it had");
    assert_eq!(
        found.iter().collect::<HashSet<_>>().len(),
        found.len(),
        "{found:?}"
    );
}

/// Why the read that mints writes back. Left unwritten, an id-less board would
/// come back a different board every time it was read, and a pane holding a
/// card would be told the card had moved when nothing had.
#[test]
fn a_board_read_twice_keeps_its_ids() {
    let scratch = Scratch::new("board-ids");
    let dir = scratch.store().init().unwrap().join("boards");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("1757000000000.toml"),
        "name = \"Roadmap\"\n\
         \n[[columns]]\nname = \"Todo\"\n\
         \n[[columns.cards]]\ntext = \"Retire Spot\"\n\
         \n[[columns.cards]]\ntext = \"Wire the model picker\"\n",
    )
    .unwrap();

    let first = ids(&scratch.store().boards()[0]);
    let second = ids(&scratch.store().boards()[0]);

    assert!(first.iter().all(|id| !id.is_empty()), "{first:?}");
    assert_eq!(first, second);
}

/// A card carries its session across, so a lane change does not lose the agent
/// working on it.
#[test]
fn moving_a_card_takes_its_session_with_it() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let todo = board.add_column("Todo").id.clone();
    let doing = board.add_column("Doing").id.clone();
    let card = board
        .add_card(&todo, "Retire Spot".into())
        .unwrap()
        .id
        .clone();
    board.dispatch_card(&card, "1757000009999".into());

    assert!(board.move_card(&card, &doing));
    assert!(board.column(&todo).unwrap().cards.is_empty());
    assert_eq!(board.column(&doing).unwrap().cards.len(), 1);
    assert_eq!(
        board.card(&card).unwrap().session.as_deref(),
        Some("1757000009999")
    );
}

/// A lane holding work cannot be dropped. The cards are the work and the
/// column is only where they sit, so there is no reading of "delete this
/// column" that means "and the cards in it".
#[test]
fn a_column_holding_cards_will_not_be_dropped() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let todo = board.add_column("Todo").id.clone();
    let card = board
        .add_card(&todo, "Retire Spot".into())
        .unwrap()
        .id
        .clone();

    assert!(!board.remove_column(&todo), "refused while it holds one");
    assert!(board.column(&todo).is_some());

    board.remove_card(&card);
    assert!(board.remove_column(&todo), "and taken once it is empty");
    assert!(board.columns.is_empty());
}

/// Naming a lane leaves its id where it was — the id is what an agent holds
/// while you rename the column under it.
#[test]
fn renaming_a_column_leaves_its_id_alone() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let id = board.add_column("Todo").id.clone();

    assert!(board.rename_column(&id, "Backlog"));
    assert_eq!(board.column(&id).unwrap().name, "Backlog");
    assert!(!board.rename_column("nobody", "Backlog"));
}

/// A card that was never dispatched has no key for a session, rather than an
/// empty one every reader has to know to expect.
#[test]
fn an_undispatched_card_has_no_session_key() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let todo = board.add_column("Todo").id.clone();
    board.add_card(&todo, "Retire Spot".into());

    let body = toml::to_string_pretty(&board).unwrap();
    assert!(!body.contains("session"), "{body}");
}

/// And the link a dispatch writes outlives the launch that wrote it, which is
/// the whole of why the field is on the file at all.
#[test]
fn a_dispatched_card_keeps_its_session() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    let todo = board.add_column("Doing").id.clone();
    let card = board
        .add_card(&todo, "Retire Spot".into())
        .unwrap()
        .id
        .clone();
    board.dispatch_card(&card, "1757000009999".into());

    let body = toml::to_string_pretty(&board).unwrap();
    let back: Board = toml::from_str(&body).unwrap();
    assert_eq!(
        back.card(&card).unwrap().session.as_deref(),
        Some("1757000009999")
    );
}

/// A column of cards, none of them named yet — what a board written before
/// ids is read as.
fn unnamed(name: &str, cards: usize) -> Column {
    let mut column = Column::new(String::new(), name);
    column.cards = (0..cards)
        .map(|n| Card::new(String::new(), format!("card {n}")))
        .collect();
    column
}

/// Every id on the board, columns and cards together, in the order they sit.
fn ids(board: &Board) -> Vec<String> {
    board
        .columns
        .iter()
        .flat_map(|column| {
            std::iter::once(&column.id).chain(column.cards.iter().map(|card| &card.id))
        })
        .cloned()
        .collect()
}
