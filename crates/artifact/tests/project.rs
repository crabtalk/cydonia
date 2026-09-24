//! Articles and numbers through the backend trait, over the filesystem backend.

mod common;

use common::Scratch;
use cydonia_artifact::{article::properties::Properties, project::Project as _};
use std::fs;

#[test]
fn an_article_round_trips_by_id() {
    let scratch = Scratch::new("project-article");
    let store = scratch.store();
    let made = store.create_article("# Plan\n").unwrap();
    assert_eq!(store.read_article(&made.id).unwrap(), "# Plan\n");
    store.write_article(&made.id, "# Plan\n\nMore.\n").unwrap();
    assert_eq!(store.read_article(&made.id).unwrap(), "# Plan\n\nMore.\n");
    let listed = store.articles();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, made.id);
}

#[test]
fn properties_save_keeps_unknown_keys() {
    let scratch = Scratch::new("project-properties");
    let store = scratch.store();
    let made = store.create_article("").unwrap();
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
    };
    store.save_properties(&made.id, &wanted).unwrap();
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
    let made = store.create_article("").unwrap();
    store.put_asset(&made.id, "a.png", b"png").unwrap();
    assert_eq!(store.asset(&made.id, "a.png").unwrap(), b"png");
    assert!(store.put_asset(&made.id, "../escape.png", b"x").is_err());
    assert!(store.asset(&made.id, "..").is_err());
}

#[test]
fn a_missing_article_is_an_error() {
    let scratch = Scratch::new("project-missing");
    let store = scratch.store();
    assert!(store.read_article("1").is_err());
    assert!(store.write_article("1", "x").is_err());
    assert!(store.read_article("../boards").is_err());
    assert_eq!(store.properties("1"), Properties::default());
}

#[test]
fn removing_an_article_retires_its_number() {
    let scratch = Scratch::new("project-remove");
    let store = scratch.store();
    let made = store.create_article("").unwrap();
    let number = store.number("article", &made.id).unwrap();
    assert_eq!(
        store.resolve("article", number).unwrap().as_deref(),
        Some(made.id.as_str())
    );
    store.remove_article(&made.id).unwrap();
    assert!(store.articles().is_empty());
    assert_eq!(store.resolve("article", number).unwrap(), None);
}

#[test]
fn a_retired_number_is_not_reissued() {
    let scratch = Scratch::new("project-retire");
    let store = scratch.store();
    let first = store.number("board", "a").unwrap();
    store.retire("board", "a").unwrap();
    assert_ne!(store.number("board", "a").unwrap(), first);
}

#[test]
fn removing_what_is_not_there_is_an_error() {
    let scratch = Scratch::new("project-absent");
    let store = scratch.store();
    assert!(store.remove_board("nope").is_err());
    assert!(store.remove_session("nope").is_err());
    assert!(store.remove_article("nope").is_err());
}
