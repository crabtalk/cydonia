//! The in-memory backend: seeded from a `.cydonia/` layout, and answering the
//! trait the way the filesystem backend does.

mod common;

use common::Wait as _;

use cydonia_artifact::kind::Kind;
use cydonia_artifact::{
    article::properties::Properties,
    board::Page,
    entry,
    project::{Owner, Project as _, memory},
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
    assert_eq!(
        store
            .asset(&Owner::Article("1790089015001".into()), "a.png")
            .wait()
            .unwrap(),
        b"png"
    );
}

#[test]
fn a_seed_holds_the_pictures_of_a_boards_pages_and_the_shared_ones() {
    let store = memory::Project::seed([
        (
            "boards/1790266971002/board.toml",
            b"name = \"Ops\"\n".as_slice(),
        ),
        (
            "boards/1790266971002/assets/d.png",
            b"description".as_slice(),
        ),
        (
            "boards/1790266971002/cards/c1/content.md",
            b"A card".as_slice(),
        ),
        (
            "boards/1790266971002/cards/c1/assets/c.png",
            b"card".as_slice(),
        ),
        ("assets/s.png", b"shared".as_slice()),
    ]);
    let asset = |owner: Owner, name: &str| store.asset(&owner, name).wait().unwrap();
    let page = |page| Owner::Page {
        board: "1790266971002".into(),
        page,
    };
    assert_eq!(asset(page(Page::Description), "d.png"), b"description");
    assert_eq!(asset(page(Page::Card("c1".into())), "c.png"), b"card");
    assert_eq!(asset(Owner::Shared, "s.png"), b"shared");
}

#[test]
fn assets_round_trip_for_every_owner_and_go_with_it() {
    let store = memory::Project::new();
    let mut board = store.create_board("Plans", "").wait().unwrap();
    let column = board.add_column("TODO").id.clone();
    let card = board.add_card(&column, "x".into()).unwrap().id.clone();
    store.save_board(&mut board).wait().unwrap();
    let article = store.create_article("").wait().unwrap();
    let on_card = Owner::Page {
        board: board.id.clone(),
        page: Page::Card(card.clone()),
    };
    let owners = [
        Owner::Article(article.id.clone()),
        on_card.clone(),
        Owner::Page {
            board: board.id.clone(),
            page: Page::Description,
        },
        Owner::Shared,
    ];
    for owner in &owners {
        store.put_asset(owner, "a.png", b"png").wait().unwrap();
        assert_eq!(store.asset(owner, "a.png").wait().unwrap(), b"png");
        assert!(store.put_asset(owner, "../a.png", b"x").wait().is_err());
        assert!(store.asset(owner, "b.png").wait().is_err());
    }

    let other = memory::Project::new();
    let landing = other.create_board("Elsewhere", "").wait().unwrap();
    store
        .carry_card_files(&board.id, &card, &other, &landing.id, "c2")
        .wait()
        .unwrap();
    let carried = Owner::Page {
        board: landing.id.clone(),
        page: Page::Card("c2".into()),
    };
    assert_eq!(other.asset(&carried, "a.png").wait().unwrap(), b"png");

    board.remove_card(&card);
    store.save_board(&mut board).wait().unwrap();
    assert!(store.asset(&on_card, "a.png").wait().is_err());
    store.remove_article(&article.id).wait().unwrap();
    assert!(store.asset(&owners[0], "a.png").wait().is_err());
    store.remove_board(&board.id).wait().unwrap();
    assert!(store.asset(&owners[2], "a.png").wait().is_err());
    assert_eq!(store.asset(&Owner::Shared, "a.png").wait().unwrap(), b"png");
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
        comments: None,
        labels: vec!["research".into()],
    };
    store
        .save_properties("1790089015001", &wanted)
        .wait()
        .unwrap();
    assert_eq!(store.properties("1790089015001"), wanted);
    let cleared = Properties::default();
    store
        .save_properties("1790089015001", &cleared)
        .wait()
        .unwrap();
    assert_eq!(store.properties("1790089015001"), cleared);
}

#[test]
fn writes_land_and_removals_retire_numbers() {
    let store = memory::Project::new();
    let mut board = store.create_board("Plans", "").wait().unwrap();
    board.name = "Plans, renamed".into();
    store.save_board(&mut board).wait().unwrap();
    assert_eq!(store.board(&board.id).unwrap().name, "Plans, renamed");

    let article = store.create_article("x").wait().unwrap();
    store.write_article(&article.id, "y").wait().unwrap();
    assert_eq!(store.read_article(&article.id).unwrap(), "y");
    let number = store.number(Kind::Article, &article.id).wait().unwrap();
    store.remove_article(&article.id).wait().unwrap();
    assert_eq!(store.resolve(Kind::Article, number).unwrap(), None);
    assert_ne!(
        store.number(Kind::Article, &article.id).wait().unwrap(),
        number
    );

    assert!(store.remove_board("nope").wait().is_err());
    assert!(store.read_article("nope").is_err());
    assert!(
        store
            .put_asset(&Owner::Article(board.id.clone()), "a.png", b"")
            .wait()
            .is_err()
    );
}

#[test]
fn a_number_resolves_only_under_its_own_kind() {
    let store = memory::Project::new();
    let number = store.number(Kind::Board, "a").wait().unwrap();
    assert_eq!(
        store.resolve(Kind::Board, number).unwrap().as_deref(),
        Some("a")
    );
    assert_eq!(store.resolve(Kind::Article, number).unwrap(), None);
}

#[test]
fn nothing_is_watched() {
    let store = memory::Project::new();
    assert!(store.watch(|| {}).is_none());
}

#[test]
fn a_seeded_entry_was_created_at_the_stamp_it_is_named_for() {
    let store = seeded();
    assert_eq!(store.boards()[0].created, Some(1790266971001));
    assert_eq!(store.articles()[0].created, Some(1790089015001));
}
