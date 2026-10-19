//! Articles and numbers through the backend trait, over the filesystem backend.

mod common;
use common::Wait as _;

use common::Scratch;
use cydonia_artifact::{
    article::properties::{CommentDisplay, Properties},
    board::Page,
    kind::Kind,
    project::{Owner, Project as _},
};
use std::fs;

#[test]
fn an_article_round_trips_by_id() {
    let scratch = Scratch::new("project-article");
    let store = scratch.store();
    let made = store.create_article("# Plan\n").wait().unwrap();
    assert_eq!(store.read_article(&made.id).unwrap(), "# Plan\n");
    store
        .write_article(&made.id, "# Plan\n\nMore.\n")
        .wait()
        .unwrap();
    assert_eq!(store.read_article(&made.id).unwrap(), "# Plan\n\nMore.\n");
    let listed = store.articles();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, made.id);
}

#[test]
fn properties_save_keeps_unknown_keys() {
    let scratch = Scratch::new("project-properties");
    let store = scratch.store();
    let made = store.create_article("").wait().unwrap();
    let file = scratch
        .path()
        .join(".cydonia/articles")
        .join(&made.id)
        .join("properties.toml");
    fs::write(&file, "owner = \"ops\"\n").unwrap();
    let wanted = Properties {
        title: "Rate limiting plan".into(),
        archived: true,
        full_width: Some(false),
        comments: Some(CommentDisplay::Minimal),
        labels: vec!["q3".into(), "research".into()],
    };
    store.save_properties(&made.id, &wanted).wait().unwrap();
    assert_eq!(store.properties(&made.id), wanted);
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("owner = \"ops\"")
    );
    assert_eq!(store.articles()[0].title, "Rate limiting plan");
}

#[test]
fn assets_are_named_blobs() {
    let scratch = Scratch::new("project-assets");
    let store = scratch.store();
    let made = store.create_article("").wait().unwrap();
    let owner = Owner::Article(made.id.clone());
    store.put_asset(&owner, "a.png", b"png").wait().unwrap();
    assert_eq!(store.asset(&owner, "a.png").wait().unwrap(), b"png");
    assert!(
        store
            .put_asset(&owner, "../escape.png", b"x")
            .wait()
            .is_err()
    );
    assert!(store.asset(&owner, "..").wait().is_err());
}

#[test]
fn a_boards_pages_and_the_shared_assets_are_where_their_documents_are() {
    let scratch = Scratch::new("project-page-assets");
    let store = scratch.store();
    let mut board = store.create_board("Plans", "").wait().unwrap();
    let column = board.add_column("TODO").id.clone();
    let card = board.add_card(&column, "x".into()).unwrap().id.clone();
    store.save_board(&mut board).wait().unwrap();
    let dir = scratch.path().join(".cydonia/boards").join(&board.id);
    let placed = [
        (
            Owner::Page {
                board: board.id.clone(),
                page: Page::Card(card.clone()),
            },
            dir.join("cards").join(&card).join("assets/a.png"),
        ),
        (
            Owner::Page {
                board: board.id.clone(),
                page: Page::Description,
            },
            dir.join("assets/a.png"),
        ),
        (Owner::Shared, scratch.path().join(".cydonia/assets/a.png")),
    ];
    for (owner, file) in &placed {
        store.put_asset(owner, "a.png", b"png").wait().unwrap();
        assert_eq!(fs::read(file).unwrap(), b"png");
        assert_eq!(store.asset(owner, "a.png").wait().unwrap(), b"png");
        assert!(store.put_asset(owner, "..", b"x").wait().is_err());
    }
    let elsewhere = Owner::Page {
        board: "nope".into(),
        page: Page::Description,
    };
    assert!(store.put_asset(&elsewhere, "a.png", b"png").wait().is_err());
    let escape = Owner::Page {
        board: board.id.clone(),
        page: Page::Card("../..".into()),
    };
    assert!(store.put_asset(&escape, "a.png", b"png").wait().is_err());
}

#[test]
fn the_first_shared_asset_makes_cydonia_ignored() {
    let scratch = Scratch::new("project-shared-assets");
    let store = scratch.store();
    store
        .put_asset(&Owner::Shared, "a.png", b"png")
        .wait()
        .unwrap();
    assert_eq!(
        fs::read_to_string(scratch.path().join(".cydonia/.gitignore")).unwrap(),
        "*\n"
    );
}

#[test]
fn a_missing_article_is_an_error() {
    let scratch = Scratch::new("project-missing");
    let store = scratch.store();
    assert!(store.read_article("1").is_err());
    assert!(store.write_article("1", "x").wait().is_err());
    assert!(store.read_article("../boards").is_err());
    assert_eq!(store.properties("1"), Properties::default());
}

#[test]
fn removing_an_article_retires_its_number() {
    let scratch = Scratch::new("project-remove");
    let store = scratch.store();
    let made = store.create_article("").wait().unwrap();
    let number = store.number(Kind::Article, &made.id).wait().unwrap();
    assert_eq!(
        store.resolve(Kind::Article, number).unwrap().as_deref(),
        Some(made.id.as_str())
    );
    store.remove_article(&made.id).wait().unwrap();
    assert!(store.articles().is_empty());
    assert_eq!(store.resolve(Kind::Article, number).unwrap(), None);
}

#[test]
fn a_retired_number_is_not_reissued() {
    let scratch = Scratch::new("project-retire");
    let store = scratch.store();
    let first = store.number(Kind::Board, "a").wait().unwrap();
    store.retire(Kind::Board, "a").wait().unwrap();
    assert_ne!(store.number(Kind::Board, "a").wait().unwrap(), first);
}

#[test]
fn removing_what_is_not_there_is_an_error() {
    let scratch = Scratch::new("project-absent");
    let store = scratch.store();
    assert!(store.remove_board("nope").wait().is_err());
    assert!(store.remove_session("nope").wait().is_err());
    assert!(store.remove_article("nope").wait().is_err());
}

#[test]
fn place_is_an_articles_directory_and_a_boards_directory() {
    let scratch = Scratch::new("project-place");
    let store = scratch.store();
    let article = store.create_article("# Plan\n").wait().unwrap();
    let board = store.create_board("Road", "").wait().unwrap();
    let cydonia = scratch.path().join(".cydonia");
    assert_eq!(
        store.place(Kind::Article, &article.id),
        Some(cydonia.join("articles").join(&article.id))
    );
    assert_eq!(
        store.place(Kind::Board, &board.id),
        Some(cydonia.join("boards").join(&board.id))
    );
    assert_eq!(store.place(Kind::Board, "missing"), None);
    assert_eq!(store.place(Kind::Table, &board.id), None);
}

#[test]
fn files_is_an_articles_a_boards_and_a_cards_folder() {
    let scratch = Scratch::new("project-files");
    let store = scratch.store();
    let cydonia = scratch.path().join(".cydonia");
    assert!(store.files(&cydonia.join("articles/1790089015001")));
    assert!(store.files(&cydonia.join("boards/1790266971001/cards/1790252301417")));
    assert!(store.files(&cydonia.join("boards/1790266971001")));
    assert!(!store.files(&cydonia.join("articles")));
    assert!(!store.files(&cydonia.join("boards/1790266971001/cards")));
    assert!(!store.files(&cydonia.join("worktrees/fix")));
    assert!(!store.files(scratch.path()));
}

#[test]
fn an_entry_was_created_at_the_stamp_it_is_named_for() {
    let scratch = Scratch::new("project-created");
    let store = scratch.store();
    let article = store.create_article("").wait().unwrap();
    let board = store.create_board("Plans", "").wait().unwrap();
    assert_eq!(article.created, article.id.parse().ok());
    assert_eq!(store.articles()[0].created, article.id.parse().ok());
    assert_eq!(
        store.board(&board.id).unwrap().created,
        board.id.parse().ok()
    );
}
