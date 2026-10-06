mod common;

use common::Scratch;
use cydonia_artifact::{
    entry::Registry,
    stats::{Stats, Tokens, word_delta},
};

#[test]
fn writes_add_into_their_day() {
    let scratch = Scratch::new("stats-upsert");
    let stats = Stats::open(scratch.path()).unwrap();
    let tokens = Tokens {
        input: 10,
        output: 5,
        cache_read: 100,
        cache_write: 1,
    };
    stats.usage("2026-10-05", "s", "m", tokens).unwrap();
    stats.usage("2026-10-05", "s", "m", tokens).unwrap();
    stats.usage("2026-10-06", "s", "m", tokens).unwrap();
    stats.message("2026-10-05", "s").unwrap();
    stats.message("2026-10-05", "s").unwrap();
    stats.words("2026-10-05", "a", 3, 1).unwrap();
    stats.card_created("2026-10-05", "c").unwrap();
    stats.card_done("2026-10-05", "c").unwrap();

    let range = stats.range("2026-10-05", "2026-10-05").unwrap();
    assert_eq!(range.usage.len(), 1);
    assert_eq!(range.usage[0].tokens.input, 20);
    assert_eq!(range.usage[0].tokens.cache_read, 200);
    assert_eq!(range.messages[0].count, 2);
    assert_eq!((range.words[0].added, range.words[0].removed), (3, 1));
    assert_eq!((range.cards[0].created, range.cards[0].done), (1, 1));
    assert_eq!(
        stats.range("2026-10-01", "2026-10-31").unwrap().usage.len(),
        2
    );
}

#[test]
fn entries_db_becomes_state_db_with_its_numbers() {
    let scratch = Scratch::new("stats-migrate");
    let mut registry = Registry::open(scratch.path()).unwrap();
    let number = registry.number("article", "kept").unwrap();
    drop(registry);
    let dir = scratch.path().join(".cydonia");
    std::fs::rename(dir.join("state.db"), dir.join("entries.db")).unwrap();

    let mut registry = Registry::open(scratch.path()).unwrap();
    assert_eq!(registry.number("article", "kept").unwrap(), number);
    assert!(dir.join("state.db").is_file());
    assert!(!dir.join("entries.db").exists());
}

#[test]
fn word_delta_counts_changed_words_only() {
    assert_eq!(word_delta("", "one two three"), (3, 0));
    assert_eq!(word_delta("one two three", "three two one"), (0, 0));
    assert_eq!(word_delta("one two", "one three four"), (2, 1));
}

#[test]
fn the_store_records_words_and_cards() {
    use cydonia_artifact::{board::Status, project::Project as _};
    let scratch = Scratch::new("stats-store");
    let store = scratch.store();
    let article = store.create_article("one two").unwrap();
    store.write_article(&article.id, "one two three").unwrap();

    let mut board = store.create_board("Work", "").unwrap();
    board.add_column("Todo");
    let column = board.columns[0].id.clone();
    let card = board.add_card(&column, "task".into()).unwrap().id.clone();
    store.save_board(&mut board).unwrap();
    board.set_card_status(&card, Some(Status::Done));
    store.save_board(&mut board).unwrap();
    store.save_board(&mut board).unwrap();

    let day = cydonia_artifact::stats::today();
    let range = Stats::open(scratch.path())
        .unwrap()
        .range(&day, &day)
        .unwrap();
    assert_eq!(range.words.len(), 1);
    assert_eq!((range.words[0].added, range.words[0].removed), (3, 0));
    assert_eq!(range.cards.len(), 1);
    assert_eq!((range.cards[0].created, range.cards[0].done), (1, 1));
}
