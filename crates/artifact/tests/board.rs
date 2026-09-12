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

/// Cards are numbered from one, in the order they are made, and the number is
/// said with the board's key.
#[test]
fn cards_are_handled_in_the_order_they_are_made() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    board.key = "ROA".into();
    let todo = board.add_column("Todo").id.clone();
    let first = board
        .add_card(&todo, "Retire Spot".into())
        .unwrap()
        .id
        .clone();
    let second = board
        .add_card(&todo, "Wire the picker".into())
        .unwrap()
        .id
        .clone();

    let handle = |id: &str| board.handle_of(board.card(id).unwrap()).unwrap();
    assert_eq!(handle(&first), "ROA-1");
    assert_eq!(handle(&second), "ROA-2");
}

/// The counter only climbs. A deleted ROAD-12 leaves a gap rather than coming
/// back as somebody else's card — a handle already said out loud has to go on
/// meaning what it meant.
#[test]
fn a_deleted_handle_never_comes_back() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    board.key = "ROA".into();
    let todo = board.add_column("Todo").id.clone();
    let first = board.add_card(&todo, "one".into()).unwrap().id.clone();
    board.add_card(&todo, "two".into());

    board.remove_card(&first);
    let third = board.add_card(&todo, "three".into()).unwrap().id.clone();

    assert_eq!(
        board.handle_of(board.card(&third).unwrap()).unwrap(),
        "ROA-3"
    );
}

/// A board keeps its key when it is renamed. The key was derived from the name
/// once; re-deriving it would break every handle written down since.
#[test]
fn renaming_a_board_leaves_its_key_alone() {
    let mut board = Board::new("1757000000000".into(), "Roadmap");
    board.key = "ROA".into();
    board.name = "Backlog".into();
    assert_eq!(board.key, "ROA");
}

/// Cards a read finds unnumbered are numbered, and numbered the same the second
/// time — the same reason the ids are written back.
#[test]
fn a_board_read_twice_keeps_its_handles() {
    let scratch = Scratch::new("board-handles");
    let dir = scratch.store().init().unwrap().join("boards");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("1757000000000.toml"),
        "name = \"Roadmap\"\n\n[[columns]]\nname = \"Todo\"\n\n[[columns.cards]]\ntext = \"one\"\n\n[[columns.cards]]\ntext = \"two\"\n",
    )
    .unwrap();

    let first = handles(&scratch.store().boards()[0]);
    let second = handles(&scratch.store().boards()[0]);
    assert_eq!(first, vec!["ROA-1", "ROA-2"]);
    assert_eq!(first, second);
}

/// Two boards in one project cannot answer to the same name.
#[test]
fn boards_in_a_project_take_different_keys() {
    let scratch = Scratch::new("board-keys");
    let one = scratch.store().create_board().expect("made");
    let two = scratch.store().create_board().expect("made");

    assert!(!one.key.is_empty());
    assert_ne!(one.key, two.key);
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
        .map(|n| Card {
            id: String::new(),
            handle: None,
            text: format!("card {n}"),
            session: None,
        })
        .collect();
    column
}

/// What every card on the board is called out loud, in the order they sit.
fn handles(board: &Board) -> Vec<String> {
    board
        .columns
        .iter()
        .flat_map(|column| column.cards.iter())
        .filter_map(|card| board.handle_of(card))
        .collect()
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
