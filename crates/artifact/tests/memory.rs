//! The in-memory backend: seeded from a `.cydonia/` layout, and answering the
//! trait the way the filesystem backend does.

use cydonia_artifact::{
    article::properties::Properties,
    entry,
    project::{Project as _, memory},
};

const BOARD: &str = r#"
name = "Roadmap"
key = "ROAD"

[[columns]]
name = "TODO"

[[columns.cards]]
text = "Token bucket middleware"
"#;

const SESSION: &str =
    r#"{"agent":"Claude Agent","title":"Plan rate limiting","closed":true,"updated":1,"items":[]}"#;

fn seeded() -> memory::Project {
    memory::Project::seed([
        ("boards/1790266971001.toml", BOARD.as_bytes()),
        ("sessions/1790252287183.json", SESSION.as_bytes()),
        (
            "articles/1790089015001/content.md",
            b"## Why now\n".as_slice(),
        ),
        (
            "articles/1790089015001/properties.toml",
            b"title = \"Rate limiting plan\"\nowner = \"ops\"\n".as_slice(),
        ),
        ("articles/1790089015001/assets/a.png", b"png".as_slice()),
        ("data.db", b"not read".as_slice()),
        ("layouts/x.toml", b"not read".as_slice()),
    ])
}

#[test]
fn a_seed_reads_like_the_files_it_came_from() {
    let store = seeded();
    let boards = store.boards();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].id, "1790266971001");
    assert_eq!(boards[0].key, "ROAD");
    assert!(boards[0].number.is_some());
    assert!(
        !boards[0].columns[0].id.is_empty(),
        "ids are minted on seed"
    );

    let sessions = store.sessions();
    assert_eq!(sessions[0].id, "1790252287183");
    assert_eq!(sessions[0].title, "Plan rate limiting");

    let articles = store.articles();
    assert_eq!(articles[0].id, "1790089015001");
    assert_eq!(articles[0].title, "Rate limiting plan");
    assert_eq!(store.read_article("1790089015001").unwrap(), "## Why now\n");
    assert_eq!(store.asset("1790089015001", "a.png").unwrap(), b"png");
}

#[test]
fn the_catalog_numbers_everything_once() {
    let store = seeded();
    let first = entry::catalog(&store).unwrap();
    let again = entry::catalog(&store).unwrap();
    assert_eq!(first.len(), 3);
    let numbers = |entries: &[entry::Entry]| {
        let mut numbers: Vec<u64> = entries.iter().map(|entry| entry.number).collect();
        numbers.sort();
        numbers
    };
    assert_eq!(numbers(&first), numbers(&again));
    for found in &first {
        assert!(entry::open(&store, found).unwrap().is_some());
    }
}

#[test]
fn properties_round_trip_and_clear() {
    let store = seeded();
    let wanted = Properties {
        title: "Renamed".into(),
        archived: true,
        full_width: None,
    };
    store.save_properties("1790089015001", &wanted).unwrap();
    assert_eq!(store.properties("1790089015001"), wanted);
    let cleared = Properties::default();
    store.save_properties("1790089015001", &cleared).unwrap();
    assert_eq!(store.properties("1790089015001"), cleared);
}

#[test]
fn writes_land_and_removals_retire_numbers() {
    let store = memory::Project::new();
    let mut board = store.create_board("Plans", "").unwrap();
    board.name = "Plans, renamed".into();
    store.save_board(&mut board).unwrap();
    assert_eq!(store.board(&board.id).unwrap().name, "Plans, renamed");

    let article = store.create_article("x").unwrap();
    store.write_article(&article.id, "y").unwrap();
    assert_eq!(store.read_article(&article.id).unwrap(), "y");
    let number = store.number("article", &article.id).unwrap();
    store.remove_article(&article.id).unwrap();
    assert_eq!(store.resolve("article", number).unwrap(), None);
    assert_ne!(store.number("article", &article.id).unwrap(), number);

    assert!(store.remove_board("nope").is_err());
    assert!(store.read_article("nope").is_err());
    assert!(store.put_asset(&board.id, "a.png", b"").is_err());
}

#[test]
fn a_number_resolves_only_under_its_own_kind() {
    let store = memory::Project::new();
    let number = store.number("board", "a").unwrap();
    assert_eq!(
        store.resolve("board", number).unwrap().as_deref(),
        Some("a")
    );
    assert_eq!(store.resolve("article", number).unwrap(), None);
}

#[test]
fn nothing_is_watched() {
    let store = memory::Project::new();
    assert!(store.watch(std::sync::Arc::new(|| {})).is_none());
}
